#!/usr/bin/env bash
#
# Tests scripts/build-sdk-rn-ios.sh, which builds rust/fedimint-sdk as a static library for each
# iOS target with cargo, then has ubrn assemble the xcframework and generate the bindings from
# those libraries.
#
#   scripts/test-build-sdk-rn-ios.sh
#
# The script needs macOS with Xcode, so it cannot run for real here. Each case instead runs a copy
# of it, inside a throwaway directory tree that mimics the repository, against fake `cargo`,
# `ubrn` and `xcodebuild` programs placed on a PATH of their own. The fake cargo builds nothing
# and leaves a text file where the library would be, the fake ubrn writes the podspec a real one
# writes, and every fake logs each call to a file. That pins down which triples are built and
# handed to ubrn, the arguments of every call, that ubrn only runs once every library exists, and
# what the script refuses. Nothing in this checkout is touched, and nothing beyond bash and the
# POSIX tools the script uses is needed.
#
# SCRIPT_UNDER_TEST names another copy of the script to run in place of
# scripts/build-sdk-rn-ios.sh. Pointing it at a deliberately broken copy shows that the cases can
# fail.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="${SCRIPT_UNDER_TEST:-$ROOT/scripts/build-sdk-rn-ios.sh}"
# The script builds the paths it passes on, which the fakes log, with `cd` and `pwd`. The work
# directory is spelled that way too, because the cases compare the two. The two steps are
# separate so that a failing mktemp stops the test: `cd ""` can succeed and stay where it is.
WORK="$(mktemp -d)"
WORK="$(cd "$WORK" && pwd)"
trap 'rm -rf "$WORK"' EXIT

LIB_NAME="libfedimint_sdk.a"
PKG_PATH="js/react-native/react-native-bindings"
FRAMEWORKS="  s.frameworks = "
TRIPLES=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
FAKES=(cargo ubrn xcodebuild)
# The system programs the script uses. The fakes use them too.
SYSTEM_TOOLS=(bash cp dirname grep ls mkdir patch sed)

# The variables the script reads. A value from the caller's environment must not reach it, so a
# case that needs one sets it.
unset UBRN_IOS_TARGETS IPHONEOS_DEPLOYMENT_TARGET

passed=0
failed=0

pass() {
    passed=$((passed + 1))
    echo "  ok    $1"
}

fail() {
    failed=$((failed + 1))
    echo "  FAIL  $1"
    [[ -z "${2:-}" ]] || printf '        %s\n' "${2//$'\n'/$'\n'        }"
}

# expect_ok <name> <expected stdout> -- <command...>: succeeds and prints exactly that.
expect_ok() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        if [[ "$out" == "$want" ]]; then
            pass "$name"
        else
            fail "$name" "expected: $want"$'\n'"got: $out"
        fi
    else
        fail "$name" "exited non-zero: $out"
    fi
}

# expect_success <name> -- <command...>: succeeds, whatever it prints.
expect_success() {
    local name="$1" out
    shift 2
    if out="$("$@" 2>&1)"; then
        pass "$name"
    else
        fail "$name" "exited non-zero: $out"
    fi
}

# expect_fail <name> <expected text in output> -- <command...>: fails and prints that text.
expect_fail() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        fail "$name" "expected a failure, got: $out"
    elif [[ "$out" == *"$want"* ]]; then
        pass "$name"
    else
        fail "$name" "expected the error to mention: $want"$'\n'"got: $out"
    fi
}

# check <name> <condition command...>: passes when the command succeeds.
check() {
    local name="$1"
    shift
    if "$@"; then pass "$name"; else fail "$name"; fi
}

# The fake programs. Each appends one line to $FAKE_LOG: its name and arguments, a `|`, then its
# working directory and the IPHONEOS_DEPLOYMENT_TARGET it sees. The variables FAKE_CARGO_SKIP and
# FAKE_CARGO_FAIL (each names a triple), FAKE_CARGO_NO_TARGET_DIR, FAKE_UBRN_KEEP_PODSPEC and
# FAKE_UBRN_FAILS make a fake misbehave.
FAKES_DIR="$WORK/fakes"
mkdir -p "$FAKES_DIR"

cat >"$FAKES_DIR/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "cargo $* | cwd=$PWD IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET-<unset>}" \
    >>"$FAKE_LOG"
case "$1" in
    metadata)
        if [[ -n "${FAKE_CARGO_NO_TARGET_DIR:-}" ]]; then
            echo '{"packages":[],"resolve":null,"version":1}'
        else
            printf '{"packages":[],"resolve":null,"target_directory":"%s","version":1}\n' \
                "$FAKE_TARGET_DIR"
        fi
        ;;
    rustc)
        triple=""
        previous=""
        for arg in "$@"; do
            [[ "$previous" != "--target" ]] || triple="$arg"
            previous="$arg"
        done
        [[ -n "$triple" ]] || { echo "fake cargo: no --target" >&2; exit 2; }
        release="$FAKE_TARGET_DIR/$triple/release"
        mkdir -p "$release"
        if [[ "$triple" == "${FAKE_CARGO_FAIL:-}" ]]; then
            # A library is in place although the build fails, so only the exit status can stop
            # the script.
            echo "stale library" >"$release/libfedimint_sdk.a"
            echo "error: could not compile fedimint-sdk for $triple" >&2
            exit 101
        fi
        if [[ "$triple" != "${FAKE_CARGO_SKIP:-}" ]]; then
            echo "static library for $triple" >"$release/libfedimint_sdk.a"
        fi
        ;;
    *)
        echo "fake cargo: unexpected command: $*" >&2
        exit 2
        ;;
esac
EOF

# ubrn writes the podspec into its working directory, the package's, unless it is told to keep
# one that is already there. A failing ubrn fails after writing its outputs, so a script that
# patched the podspec regardless would show.
cat >"$FAKES_DIR/ubrn" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "ubrn $* | cwd=$PWD IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET-<unset>}" \
    >>"$FAKE_LOG"
if [[ -z "${FAKE_UBRN_KEEP_PODSPEC:-}" || ! -e ReactNativeBindings.podspec ]]; then
    cp "$FAKE_PODSPEC" ReactNativeBindings.podspec
fi
mkdir -p FedimintReactNativeBindingsFramework.xcframework
echo "<plist/>" >FedimintReactNativeBindingsFramework.xcframework/Info.plist
if [[ -n "${FAKE_UBRN_FAILS:-}" ]]; then
    echo "error: ubrn failed" >&2
    exit 1
fi
EOF

cat >"$FAKES_DIR/xcodebuild" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "xcodebuild $* | cwd=$PWD IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET-<unset>}" \
    >>"$FAKE_LOG"
EOF
chmod +x "$FAKES_DIR"/*

# A directory of symbolic links to the system programs, plus to the fakes except the one named
# by $2 (if any). It is the whole PATH of a run, so a missing program is really missing, also on
# a machine where Xcode's own tools are in /usr/bin. Prints its path.
path_dir() {
    local name="$1" omit="${2:-}" dir tool
    dir="$WORK/path.$name"
    mkdir -p "$dir"
    for tool in "${SYSTEM_TOOLS[@]}"; do
        ln -s "$(command -v "$tool")" "$dir/$tool"
    done
    for tool in "${FAKES[@]}"; do
        [[ "$tool" == "$omit" ]] || ln -s "$FAKES_DIR/$tool" "$dir/$tool"
    done
    echo "$dir"
}

FULL_PATH="$(path_dir full)"

# The podspec ubrn writes: the committed one without the line the patch adds, which is the
# podspec the patch has to apply to.
PODSPEC_TEMPLATE="$WORK/podspec.template"
grep -v "^$FRAMEWORKS" "$ROOT/$PKG_PATH/ReactNativeBindings.podspec" >"$PODSPEC_TEMPLATE"

# A fresh tree <case>/repo holding a copy of the script, a stand-in manifest and the files of the
# bindings package that the script uses or hands to ubrn. Next to it, in <case>, are the target
# directory the fake cargo names, which lies outside the tree so that the script has to ask cargo
# for it, and the podspec the fake ubrn writes. Prints the path of repo.
make_repo() {
    local case_dir repo pkg
    case_dir="$(mktemp -d "$WORK/case.XXXX")"
    repo="$case_dir/repo"
    pkg="$repo/$PKG_PATH"
    mkdir -p "$repo/scripts" "$repo/rust/fedimint-sdk" "$pkg/patches" "$case_dir/target"
    cp "$SCRIPT" "$repo/scripts/build-sdk-rn-ios.sh"
    echo "# Stand-in for the manifest of the SDK." >"$repo/rust/fedimint-sdk/Cargo.toml"
    cp "$ROOT/$PKG_PATH/ubrn.config.yaml" "$pkg/"
    cp "$ROOT/$PKG_PATH/patches/add_ios_frameworks.patch" "$pkg/patches/"
    cp "$PODSPEC_TEMPLATE" "$case_dir/podspec.template"
    echo "$repo"
}

# run <repo> [--path <dir>] [VAR=value...]: runs the copy of the script in <repo> from the case
# directory, with only the fakes and the system programs on PATH, and a fresh call log.
run() {
    local repo="$1" path="$FULL_PATH" case_dir
    shift
    if [[ "${1:-}" == "--path" ]]; then
        path="$2"
        shift 2
    fi
    case_dir="$(dirname "$repo")"
    : >"$case_dir/calls.log"
    (
        cd "$case_dir"
        env PATH="$path" FAKE_LOG="$case_dir/calls.log" FAKE_TARGET_DIR="$case_dir/target" \
            FAKE_PODSPEC="$case_dir/podspec.template" "$@" "$repo/scripts/build-sdk-rn-ios.sh"
    )
}

# contains <text> <needle>: succeeds when <text> has <needle> in it.
contains() {
    [[ "$1" == *"$2"* ]]
}

# The helpers below read the call log of the last run in <repo>.

# log_count <repo> <glob pattern>...: the number of lines of the log that match any pattern.
log_count() {
    local log line pattern count=0
    log="$(dirname "$1")/calls.log"
    shift
    while IFS= read -r line; do
        for pattern in "$@"; do
            # shellcheck disable=SC2053 # The pattern is a glob on purpose.
            if [[ "$line" == $pattern ]]; then
                count=$((count + 1))
                break
            fi
        done
    done <"$log"
    echo "$count"
}

# calls_with <repo> <command> <words>: the number of calls of <command> (`cargo rustc`, `ubrn`)
# that have <words> in a row, as whole words, among their arguments or in the working directory
# and deployment target that are logged after them.
calls_with() {
    local line count=0
    while IFS= read -r line; do
        if [[ "$line" == "$2 "* && " $line " == *" $3 "* ]]; then count=$((count + 1)); fi
    done <"$(dirname "$1")/calls.log"
    echo "$count"
}

# calls <repo>: the calls in order, on one line: `metadata` and `rustc` for cargo, else the
# program's name.
calls() {
    local line list=""
    while IFS= read -r line; do
        case "$line" in
            "cargo metadata "*)
                list="${list:+$list }metadata"
                ;;
            "cargo rustc "*)
                list="${list:+$list }rustc"
                ;;
            *)
                list="${list:+$list }${line%% *}"
                ;;
        esac
    done <"$(dirname "$1")/calls.log"
    echo "$list"
}

# built <repo>: the triples cargo was asked to build, in order, on one line.
built() {
    local line rest list=""
    while IFS= read -r line; do
        if [[ "$line" == "cargo rustc "* ]]; then
            rest="${line#* --target }"
            list="${list:+$list }${rest%% *}"
        fi
    done <"$(dirname "$1")/calls.log"
    echo "$list"
}

# ubrn_targets <repo>: the value of --targets in every call of ubrn, one per line.
ubrn_targets() {
    local line rest
    while IFS= read -r line; do
        if [[ "$line" == "ubrn "* ]]; then
            rest="${line#* --targets }"
            echo "${rest%% *}"
        fi
    done <"$(dirname "$1")/calls.log"
}

# count_lines <file> <prefix>: the number of lines of the file that start with the prefix.
count_lines() {
    local line count=0
    while IFS= read -r line || [[ -n "$line" ]]; do
        if [[ "$line" == "$2"* ]]; then count=$((count + 1)); fi
    done <"$1"
    echo "$count"
}

echo "the podspec of the fake ubrn"

check "the committed podspec has the frameworks line once, which the fake ubrn's lacks" \
    test "$(count_lines "$ROOT/$PKG_PATH/ReactNativeBindings.podspec" "$FRAMEWORKS")" = 1

echo
echo "the default list"

r="$(make_repo)"
pkg="$r/$PKG_PATH"
manifest="$r/rust/fedimint-sdk/Cargo.toml"
builds="${#TRIPLES[@]}"
status=0
out="$(run "$r" 2>&1)" || status=$?
if [[ "$status" -eq 0 ]]; then
    pass "building without a list succeeds"
else
    fail "building without a list succeeds" "$out"
fi
expect_ok "cargo builds the device, the arm64 simulator and the x86_64 simulator, in this order" \
    "aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios" -- built "$r"
expect_ok "ubrn gets exactly the triples that were built" \
    "aarch64-apple-ios,aarch64-apple-ios-sim,x86_64-apple-ios" -- ubrn_targets "$r"
expect_ok "the target directory is asked for first, ubrn runs once and last" \
    "metadata rustc rustc rustc ubrn" -- calls "$r"
for option in --locked --lib --release "--features uniffi" "--crate-type lib" \
    "--crate-type staticlib"; do
    expect_ok "every cargo rustc call has $option" "$builds" -- \
        calls_with "$r" "cargo rustc" "$option"
done
expect_ok "every cargo rustc call is for the manifest of the SDK" "$builds" -- \
    calls_with "$r" "cargo rustc" "--manifest-path $manifest"
expect_ok "every cargo rustc call runs in rust/" "$builds" -- \
    calls_with "$r" "cargo rustc" "cwd=$r/rust"
expect_ok "cargo metadata is for the manifest of the SDK" 1 -- \
    calls_with "$r" "cargo metadata" "--manifest-path $manifest"
expect_ok "cargo metadata asks for format version 1" 1 -- \
    calls_with "$r" "cargo metadata" "--format-version 1"
expect_ok "cargo metadata runs in rust/" 1 -- calls_with "$r" "cargo metadata" "cwd=$r/rust"
for option in "build ios" "--config ubrn.config.yaml" --release --no-cargo --and-generate; do
    expect_ok "ubrn has $option" 1 -- calls_with "$r" ubrn "$option"
done
expect_ok "ubrn runs in the package directory" 1 -- calls_with "$r" ubrn "cwd=$pkg"
check "the summary says it is done" contains "$out" "==> Done."
check "the summary lists what the xcframework holds" contains "$out" "Info.plist"

# CI passes an empty value when it wants all the slices.
r="$(make_repo)"
expect_success "building with an empty list succeeds" -- run "$r" UBRN_IOS_TARGETS=
expect_ok "cargo builds the default triples for an empty list" \
    "aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios" -- built "$r"
expect_ok "ubrn gets the default triples for an empty list" \
    "aarch64-apple-ios,aarch64-apple-ios-sim,x86_64-apple-ios" -- ubrn_targets "$r"

echo
echo "a narrowed list"

r="$(make_repo)"
d="$(dirname "$r")"
# The libraries of the other two triples are lying around, as after a full build.
for triple in aarch64-apple-ios-sim x86_64-apple-ios; do
    mkdir -p "$d/target/$triple/release"
    echo "stale library" >"$d/target/$triple/release/$LIB_NAME"
done
expect_success "building the device slice alone succeeds" -- \
    run "$r" UBRN_IOS_TARGETS=aarch64-apple-ios
expect_ok "cargo builds that one triple only" "aarch64-apple-ios" -- built "$r"
expect_ok "ubrn gets that one triple, not the libraries lying around" "aarch64-apple-ios" -- \
    ubrn_targets "$r"

r="$(make_repo)"
expect_success "building two triples succeeds" -- \
    run "$r" UBRN_IOS_TARGETS=x86_64-apple-ios,aarch64-apple-ios-sim
expect_ok "cargo builds them in the order given" "x86_64-apple-ios aarch64-apple-ios-sim" -- \
    built "$r"
expect_ok "ubrn gets them in the order given" "x86_64-apple-ios,aarch64-apple-ios-sim" -- \
    ubrn_targets "$r"

echo
echo "the deployment target"

r="$(make_repo)"
expect_success "building without IPHONEOS_DEPLOYMENT_TARGET succeeds" -- run "$r"
expect_ok "cargo sees 15.0 when the caller sets none" "$builds" -- \
    calls_with "$r" "cargo rustc" "IPHONEOS_DEPLOYMENT_TARGET=15.0"

r="$(make_repo)"
expect_success "building with IPHONEOS_DEPLOYMENT_TARGET succeeds" -- \
    run "$r" IPHONEOS_DEPLOYMENT_TARGET=17.2
expect_ok "cargo sees the value of the caller" "$builds" -- \
    calls_with "$r" "cargo rustc" "IPHONEOS_DEPLOYMENT_TARGET=17.2"

r="$(make_repo)"
expect_success "building with an empty IPHONEOS_DEPLOYMENT_TARGET succeeds" -- \
    run "$r" IPHONEOS_DEPLOYMENT_TARGET=
expect_ok "cargo sees 15.0 when the value of the caller is empty" "$builds" -- \
    calls_with "$r" "cargo rustc" "IPHONEOS_DEPLOYMENT_TARGET=15.0"

echo
echo "a library missing after its build"

built_so_far=""
for triple in "${TRIPLES[@]}"; do
    built_so_far="${built_so_far:+$built_so_far }$triple"
    r="$(make_repo)"
    d="$(dirname "$r")"
    expect_fail "no library for $triple fails and names the triple and where it looked" \
        "no $LIB_NAME at $d/target/$triple/release after building $triple" -- \
        run "$r" FAKE_CARGO_SKIP="$triple"
    expect_ok "the builds stop with $triple" "$built_so_far" -- built "$r"
    expect_ok "ubrn is not started when $triple has no library" 0 -- log_count "$r" 'ubrn *'
done

echo
echo "a failing build"

r="$(make_repo)"
expect_fail "a failing cargo build fails the run" \
    "error: could not compile fedimint-sdk for aarch64-apple-ios-sim" -- \
    run "$r" FAKE_CARGO_FAIL=aarch64-apple-ios-sim
expect_ok "the builds stop with the failing one" "aarch64-apple-ios aarch64-apple-ios-sim" -- \
    built "$r"
expect_ok "ubrn is not started after a failing build, even if that build left a library" 0 -- \
    log_count "$r" 'ubrn *'

echo
echo "UBRN_IOS_TARGETS"

for bad in "aarch64-apple-ios, aarch64-apple-ios-sim" "aarch64-apple-ios,,x86_64-apple-ios" \
    ",aarch64-apple-ios" "aarch64-apple-ios x86_64-apple-ios" \
    "aarch64-apple-ios;x86_64-apple-ios" "Aarch64-apple-ios"; do
    r="$(make_repo)"
    message="UBRN_IOS_TARGETS is '$bad': it must be target triples separated by commas,"
    expect_fail "'$bad' is refused with the message" "$message without spaces" -- \
        run "$r" UBRN_IOS_TARGETS="$bad"
    expect_ok "'$bad' starts neither a build nor ubrn" 0 -- \
        log_count "$r" 'cargo rustc *' 'ubrn *'
done

# `read -a` reads one line, so a value that starts with a newline leaves it nothing to split.
r="$(make_repo)"
expect_fail "a value without any triple is refused" "UBRN_IOS_TARGETS names no target" -- \
    run "$r" UBRN_IOS_TARGETS=$'\n'
expect_ok "a value without any triple starts neither a build nor ubrn" 0 -- \
    log_count "$r" 'cargo rustc *' 'ubrn *'

# `read -a` drops the empty field after a trailing comma, so no empty triple is left to refuse.
r="$(make_repo)"
expect_success "a trailing comma is accepted" -- \
    run "$r" UBRN_IOS_TARGETS=aarch64-apple-ios,x86_64-apple-ios,
expect_ok "a trailing comma names no further triple to cargo" \
    "aarch64-apple-ios x86_64-apple-ios" -- built "$r"
expect_ok "a trailing comma does not reach ubrn" "aarch64-apple-ios,x86_64-apple-ios" -- \
    ubrn_targets "$r"

echo
echo "the tools"

for tool in "${FAKES[@]}"; do
    r="$(make_repo)"
    expect_fail "without $tool on PATH the script fails and names it" \
        "$tool not on PATH; run in the .#ios shell on macOS" -- \
        run "$r" --path "$(path_dir "no-$tool" "$tool")"
    expect_ok "without $tool on PATH nothing is run" 0 -- log_count "$r" '*'
done

# What `pnpm run` does: it puts node_modules/.bin, with the ubrn of the npm package, on PATH
# ahead of the ubrn of the nix shell.
r="$(make_repo)"
d="$(dirname "$r")"
mkdir -p "$d/node_modules/.bin"
ln -s "$FAKES_DIR/ubrn" "$d/node_modules/.bin/ubrn"
expect_fail "a ubrn under node_modules is refused with the message" \
    "ubrn resolves to the npm package's CLI ($d/node_modules/.bin/ubrn)" -- \
    run "$r" --path "$d/node_modules/.bin:$FULL_PATH"
expect_ok "with a ubrn under node_modules nothing is run, cargo included" 0 -- \
    log_count "$r" '*'

echo
echo "the podspec"

r="$(make_repo)"
pkg="$r/$PKG_PATH"
expect_success "building patches the podspec ubrn wrote" -- run "$r"
expect_ok "the podspec has the frameworks line once" 1 -- \
    count_lines "$pkg/ReactNativeBindings.podspec" "$FRAMEWORKS"

# ubrn keeps a podspec that is already there, here the committed one, which has the frameworks
# line. `patch --forward` fails on a hunk that is already applied, so the script must skip it.
r="$(make_repo)"
pkg="$r/$PKG_PATH"
cp "$ROOT/$PKG_PATH/ReactNativeBindings.podspec" "$pkg/"
expect_success "a kept podspec that has the frameworks line is not patched again" -- \
    run "$r" FAKE_UBRN_KEEP_PODSPEC=1
expect_ok "the kept podspec still has the frameworks line once" 1 -- \
    count_lines "$pkg/ReactNativeBindings.podspec" "$FRAMEWORKS"
check "the kept podspec has no rejected hunk" test ! -e "$pkg/ReactNativeBindings.podspec.rej"

# A podspec that the patch does not fit stands for a ubrn with a different podspec template.
r="$(make_repo)"
d="$(dirname "$r")"
pkg="$r/$PKG_PATH"
printf 'Pod::Spec.new do |s|\n  s.name = "Other"\nend\n' >"$d/podspec.template"
expect_fail "a podspec the patch does not fit fails the run" "ReactNativeBindings.podspec" -- \
    run "$r"
expect_ok "a podspec the patch does not fit is left without the frameworks line" 0 -- \
    count_lines "$pkg/ReactNativeBindings.podspec" "$FRAMEWORKS"

r="$(make_repo)"
pkg="$r/$PKG_PATH"
expect_fail "a failing ubrn fails the run" "error: ubrn failed" -- run "$r" FAKE_UBRN_FAILS=1
expect_ok "the podspec of a failing ubrn is not patched" 0 -- \
    count_lines "$pkg/ReactNativeBindings.podspec" "$FRAMEWORKS"

echo
echo "the target directory"

r="$(make_repo)"
expect_fail "a cargo metadata without a target directory fails the run" \
    "cargo metadata named no target directory" -- run "$r" FAKE_CARGO_NO_TARGET_DIR=1
expect_ok "nothing is built without a target directory" 0 -- \
    log_count "$r" 'cargo rustc *' 'ubrn *'

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
