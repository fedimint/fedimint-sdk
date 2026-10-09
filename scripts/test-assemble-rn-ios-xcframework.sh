#!/usr/bin/env bash
#
# Tests scripts/assemble-rn-ios-xcframework.sh, which puts together the xcframework that
# @fedimint/react-native-bindings ships.
#
#   scripts/test-assemble-rn-ios-xcframework.sh
#
# The script needs Xcode, so it cannot run for real here. Each case instead runs a copy of it,
# inside a throwaway directory tree that mimics the repository, against fake `xcodebuild`, `lipo`,
# `otool` and `llvm-bitcode-strip` programs placed on a PATH of their own. The fakes work on
# small text files, where the word BITCODE stands for the embedded bitcode, and log every call to
# a file. That pins down the order of the steps, the arguments of every call and the checks on the
# result. Nothing in this checkout is touched, and nothing beyond bash and the POSIX tools the
# script uses is needed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

LIB_NAME="libfedimint_sdk.a"
BUNDLE_PATH="js/react-native/react-native-bindings/FedimintReactNativeBindingsFramework.xcframework"
TRIPLES=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
FAKES=(xcodebuild lipo otool llvm-bitcode-strip)
# The system programs the script uses. The fakes use them too.
SYSTEM_TOOLS=(bash basename cat cp cut dirname du find grep mkdir mktemp rm sed sort tr)

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

# expect_ok <name> <expected stdout> -- <command...>
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

# expect_fail <name> <expected text in output> -- <command...>
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

# expect_usage <name> -- <command...>: fails with status 2 and prints the usage lines.
expect_usage() {
    local name="$1" out status=0
    shift 2
    out="$("$@" 2>&1)" || status=$?
    if [[ "$status" -eq 2 && "$out" == *usage* ]]; then
        pass "$name"
    else
        fail "$name" "expected exit status 2 and a usage message, got status $status: $out"
    fi
}

# check <name> <condition command...>: passes when the command succeeds.
check() {
    local name="$1"
    shift
    if "$@"; then pass "$name"; else fail "$name"; fi
}

# The fake programs. Each appends one line, its name and arguments, to $FAKE_LOG. The variables
# FAKE_STRIP_NOOP, FAKE_XCODE_HEADERS and FAKE_XCODE_SIM_NAME make a fake misbehave.
FAKES_DIR="$WORK/fakes"
mkdir -p "$FAKES_DIR"

cat >"$FAKES_DIR/llvm-bitcode-strip" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "llvm-bitcode-strip $*" >>"$FAKE_LOG"
[[ "$1" == "-r" && "$3" == "-o" ]]
if [[ -n "${FAKE_STRIP_NOOP:-}" ]]; then
    cp "$2" "$4"
else
    sed 's/BITCODE//g' "$2" >"$4"
fi
EOF

cat >"$FAKES_DIR/lipo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "lipo $*" >>"$FAKE_LOG"
case "$1" in
    -create)
        [[ "$4" == "-output" ]]
        cat "$2" "$3" >"$5"
        ;;
    -archs)
        echo arm64
        ;;
    *)
        exit 1
        ;;
esac
EOF

cat >"$FAKES_DIR/otool" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "otool $*" >>"$FAKE_LOG"
[[ "$1" == "-l" ]]
if grep -q BITCODE "$2"; then
    echo "sectname __bitcode"
else
    echo "sectname __text"
fi
EOF

cat >"$FAKES_DIR/xcodebuild" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
echo "xcodebuild $*" >>"$FAKE_LOG"
[[ "$1" == "-create-xcframework" && "$2" == "-library" && "$4" == "-library" ]]
[[ "$6" == "-output" ]]
bundle="$7"
if [[ -e "$bundle" ]]; then
    echo "error: the output $bundle already exists" >&2
    exit 1
fi
sim_name="${FAKE_XCODE_SIM_NAME:-ios-arm64_x86_64-simulator}"
mkdir -p "$bundle/ios-arm64" "$bundle/$sim_name"
echo "<plist/>" >"$bundle/Info.plist"
cp "$3" "$bundle/ios-arm64/libfedimint_sdk.a"
cp "$5" "$bundle/$sim_name/libfedimint_sdk.a"
if [[ -n "${FAKE_XCODE_HEADERS:-}" ]]; then
    mkdir "$bundle/ios-arm64/Headers"
fi
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

# A fresh tree <case>/repo holding a copy of the script and the libraries a Nix build leaves
# behind, with every library listed in the manifest, plus an empty <case>/tmp for TMPDIR. The
# libraries are text files containing the word BITCODE. Prints the path of repo.
make_repo() {
    local case_dir repo target triple
    case_dir="$(mktemp -d "$WORK/case.XXXX")"
    repo="$case_dir/repo"
    target="$repo/rust/fedimint-sdk/target"
    mkdir -p "$repo/scripts" "$repo/$(dirname "$BUNDLE_PATH")" "$case_dir/tmp"
    cp "$ROOT/scripts/assemble-rn-ios-xcframework.sh" "$repo/scripts/"
    for triple in "${TRIPLES[@]}" aarch64-apple-darwin; do
        mkdir -p "$target/$triple/release"
        printf 'machine code of %s BITCODE\n' "$triple" >"$target/$triple/release/$LIB_NAME"
    done
    mkdir -p "$target/lipo-ios-sim/release"
    printf 'machine code of both simulators BITCODE\n' >"$target/lipo-ios-sim/release/$LIB_NAME"
    printf '%s\n' "${TRIPLES[@]}" aarch64-apple-darwin >"$target/apple-slices.txt"
    echo "$repo"
}

# run <repo> [VAR=value...] [-- <script arguments...>]: runs the copy of the script in <repo> with
# only the fakes and the system programs on PATH, its own TMPDIR and a fresh call log.
run() {
    local repo="$1" path="$FULL_PATH"
    shift
    if [[ "${1:-}" == "--path" ]]; then
        path="$2"
        shift 2
    fi
    local vars=()
    while [[ $# -gt 0 && "$1" != "--" ]]; do
        vars+=("$1")
        shift
    done
    [[ "${1:-}" != "--" ]] || shift
    local case_dir
    case_dir="$(dirname "$repo")"
    : >"$case_dir/calls.log"
    if [[ ${#vars[@]} -gt 0 ]]; then
        env PATH="$path" TMPDIR="$case_dir/tmp" FAKE_LOG="$case_dir/calls.log" "${vars[@]}" \
            "$repo/scripts/assemble-rn-ios-xcframework.sh" "$@"
    else
        env PATH="$path" TMPDIR="$case_dir/tmp" FAKE_LOG="$case_dir/calls.log" \
            "$repo/scripts/assemble-rn-ios-xcframework.sh" "$@"
    fi
}

# contains <text> <needle>: succeeds when <text> has <needle> in it.
contains() {
    [[ "$1" == *"$2"* ]]
}

# The number of lines of <repo>'s call log that match the glob pattern $2.
log_count() {
    local pattern="$2" log line count=0
    log="$(dirname "$1")/calls.log"
    while IFS= read -r line; do
        # shellcheck disable=SC2053 # The pattern is a glob on purpose.
        if [[ "$line" == $pattern ]]; then count=$((count + 1)); fi
    done <"$log"
    echo "$count"
}

# The names of everything in <dir>, one per line, relative to it. Nothing if it is empty.
entries() {
    (cd "$1" && find . -mindepth 1 | sort)
}

echo "happy path"

r="$(make_repo)"
d="$(dirname "$r")"
target="$r/rust/fedimint-sdk/target"
bundle="$r/$BUNDLE_PATH"
status=0
out="$(run "$r" 2>&1)" || status=$?
if [[ "$status" -eq 0 ]]; then
    pass "assembling succeeds"
else
    fail "assembling succeeds" "$out"
fi
want_entries="./Info.plist
./ios-arm64
./ios-arm64/$LIB_NAME
./ios-arm64_x86_64-simulator
./ios-arm64_x86_64-simulator/$LIB_NAME"
if [[ -d "$bundle" && "$(entries "$bundle")" == "$want_entries" ]]; then
    pass "the bundle holds Info.plist and the two libraries, nothing else"
else
    fail "the bundle holds Info.plist and the two libraries, nothing else" \
        "$(entries "$bundle" 2>&1)"
fi
check "the device library has no bitcode left" \
    test "$(grep -c BITCODE "$bundle/ios-arm64/$LIB_NAME" || true)" = 0
check "the simulator library has no bitcode left" \
    test "$(grep -c BITCODE "$bundle/ios-arm64_x86_64-simulator/$LIB_NAME" || true)" = 0
check "the device library is the stripped aarch64-apple-ios one" \
    grep -q "aarch64-apple-ios " "$bundle/ios-arm64/$LIB_NAME"
check "the simulator library is the stripped merge of both simulators" \
    grep -q "aarch64-apple-ios-sim.*x86_64-apple-ios" \
    <(tr '\n' ' ' <"$bundle/ios-arm64_x86_64-simulator/$LIB_NAME")

# The log lines of the three tools, with the temporary directory (one level below $d/tmp) as `*`.
tmp="$d/tmp/*"
check "one strip call per triple, 3 in all" \
    test "$(log_count "$r" 'llvm-bitcode-strip *')" = 3
for triple in "${TRIPLES[@]}"; do
    strip_call="llvm-bitcode-strip -r $target/$triple/release/$LIB_NAME -o $tmp/$triple/$LIB_NAME"
    check "$triple is stripped from target/$triple/release into the temporary directory" \
        test "$(log_count "$r" "$strip_call")" = 1
done
sim="$tmp/aarch64-apple-ios-sim/$LIB_NAME $tmp/x86_64-apple-ios/$LIB_NAME"
check "lipo merges the two stripped simulator libraries" \
    test "$(log_count "$r" "lipo -create $sim -output $tmp/ios-simulator/$LIB_NAME")" = 1
check "lipo -create runs once and nothing reads target/lipo-ios-sim" \
    test "$(log_count "$r" 'lipo -create *')" = 1 -a "$(log_count "$r" '*lipo-ios-sim*')" = 0
libs="-library $tmp/aarch64-apple-ios/$LIB_NAME -library $tmp/ios-simulator/$LIB_NAME"
check "xcodebuild gets the stripped device and the merged simulator library" \
    test "$(log_count "$r" "xcodebuild -create-xcframework $libs -output $bundle")" = 1
check "xcodebuild runs once and gets no -headers" \
    test "$(log_count "$r" 'xcodebuild *')" = 1 -a "$(log_count "$r" '*-headers*')" = 0
check "the summary says it is done and names the bundle" \
    contains "$out" "==> Done."$'\n'"    FedimintReactNativeBindingsFramework.xcframework"
check "the summary lists the device slice with its architectures" \
    contains "$out" "      ios-arm64                        arm64 "
check "the summary lists the simulator slice with its architectures" \
    contains "$out" "      ios-arm64_x86_64-simulator       arm64 "

echo
echo "the inputs"

unchanged=1
for triple in "${TRIPLES[@]}" aarch64-apple-darwin; do
    [[ "$(cat "$target/$triple/release/$LIB_NAME")" == "machine code of $triple BITCODE" ]] ||
        unchanged=0
done
merged="machine code of both simulators BITCODE"
[[ "$(cat "$target/lipo-ios-sim/release/$LIB_NAME")" == "$merged" ]] || unchanged=0
check "the libraries under target/ are unchanged and still hold their bitcode" test "$unchanged" = 1

echo
echo "an existing bundle"

r="$(make_repo)"
bundle="$r/$BUNDLE_PATH"
mkdir -p "$bundle/ios-arm64"
echo stale >"$bundle/ios-arm64/stale-file"
echo stale >"$bundle/stale-too"
status=0
out="$(run "$r" 2>&1)" || status=$?
if [[ "$status" -eq 0 && ! -e "$bundle/ios-arm64/stale-file" && ! -e "$bundle/stale-too" ]]; then
    pass "an existing bundle is replaced, not merged into"
else
    fail "an existing bundle is replaced, not merged into" "status $status: $out"
fi

echo
echo "the manifest and the libraries"

r="$(make_repo)"
rm "$r/rust/fedimint-sdk/target/apple-slices.txt"
expect_fail "a missing manifest names it" "apple-slices.txt" -- run "$r"
expect_fail "a missing manifest says how to make it" "nix-build-ios-lib.sh" -- run "$r"
check "a missing manifest leaves no bundle" test ! -e "$r/$BUNDLE_PATH"

for missing in "${TRIPLES[@]}"; do
    r="$(make_repo)"
    printf '%s\n' "${TRIPLES[@]}" aarch64-apple-darwin | grep -vxF "$missing" \
        >"$r/rust/fedimint-sdk/target/apple-slices.txt"
    expect_fail "a manifest without $missing names exactly that" "does not list: $missing. " -- \
        run "$r"
done

r="$(make_repo)"
printf '%s\n' aarch64-apple-ios >"$r/rust/fedimint-sdk/target/apple-slices.txt"
expect_fail "a manifest with two triples missing names both" \
    "does not list: aarch64-apple-ios-sim x86_64-apple-ios. " -- run "$r"

r="$(make_repo)"
printf '%s\n' aarch64-apple-ios aarch64-apple-ios-sim \
    >"$r/rust/fedimint-sdk/target/apple-slices.txt"
expect_fail "a manifest without x86_64-apple-ios says how to refresh it" \
    "nix-build-ios-lib.sh" -- run "$r"

r="$(make_repo)"
printf '%s\n' aarch64-apple-ios aarch64-apple-ios-sim-extra x86_64-apple-ios \
    >"$r/rust/fedimint-sdk/target/apple-slices.txt"
expect_fail "a manifest line is matched whole, not as a prefix" \
    "does not list: aarch64-apple-ios-sim. " -- run "$r"

for triple in "${TRIPLES[@]}"; do
    r="$(make_repo)"
    rm "$r/rust/fedimint-sdk/target/$triple/release/$LIB_NAME"
    expect_fail "a listed $triple without its library fails and names it" \
        "target/$triple/release/$LIB_NAME" -- run "$r"
done

r="$(make_repo)"
rm "$r/rust/fedimint-sdk/target/lipo-ios-sim/release/$LIB_NAME"
status=0
run "$r" >/dev/null 2>&1 || status=$?
check "the merged lipo-ios-sim library is not needed" test "$status" -eq 0

echo
echo "the result is read back"

r="$(make_repo)"
expect_fail "a strip that leaves bitcode behind fails" "bitcode" -- run "$r" FAKE_STRIP_NOOP=1

r="$(make_repo)"
expect_fail "a differently named simulator slice fails" "ios-arm64_x86_64-simulator" -- \
    run "$r" FAKE_XCODE_SIM_NAME=ios-arm64-simulator
r="$(make_repo)"
expect_fail "the failure lists what the bundle holds instead" "./ios-arm64-simulator/$LIB_NAME" -- \
    run "$r" FAKE_XCODE_SIM_NAME=ios-arm64-simulator

r="$(make_repo)"
expect_fail "a Headers directory in the bundle fails" "Headers" -- run "$r" FAKE_XCODE_HEADERS=1

echo
echo "the tools"

for tool in "${FAKES[@]}"; do
    r="$(make_repo)"
    expect_fail "without $tool on PATH the script fails and names it" "$tool is not on PATH" -- \
        run "$r" --path "$(path_dir "no-$tool" "$tool")"
done
r="$(make_repo)"
expect_fail "without llvm-bitcode-strip it says how to get it" \
    "nix shell .#llvm-bitcode-strip -c scripts/assemble-rn-ios-xcframework.sh" -- \
    run "$r" --path "$(path_dir no-strip-hint llvm-bitcode-strip)"
r="$(make_repo)"
expect_fail "without xcodebuild it says it comes with Xcode" "Xcode" -- \
    run "$r" --path "$(path_dir no-xcode-hint xcodebuild)"

echo
echo "usage"

r="$(make_repo)"
expect_usage "an argument is a usage error" -- run "$r" -- --bogus
check "a usage error runs nothing" test "$(wc -l <"$(dirname "$r")/calls.log" | tr -d ' ')" = 0

echo
echo "cleanup"

# The temporary directory is made under TMPDIR, which the fakes' log shows being used, so an empty
# TMPDIR afterwards means the script removed it.
r="$(make_repo)"
d="$(dirname "$r")"
status=0
run "$r" >/dev/null 2>&1 || status=$?
check "the run to check the cleanup of succeeds" test "$status" -eq 0
check "the script worked under its TMPDIR" test "$(log_count "$r" "*$d/tmp/*")" -gt 0
check "no temporary directory is left after a successful run" test -z "$(entries "$d/tmp")"

r="$(make_repo)"
d="$(dirname "$r")"
status=0
run "$r" FAKE_STRIP_NOOP=1 >/dev/null 2>&1 || status=$?
check "the run to check the cleanup of fails" test "$status" -eq 1
check "the script worked under its TMPDIR when it failed" \
    test "$(log_count "$r" "*$d/tmp/*")" -gt 0
check "no temporary directory is left after a failing run" test -z "$(entries "$d/tmp")"

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
