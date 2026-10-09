#!/usr/bin/env bash
#
# Tests the React Native release scripts. They decide what a valid release tag is, and a wrong
# tag or a wrong npm version can never be taken back, so every rule they enforce has a case here.
#
#   scripts/test-react-native-sdk-scripts.sh
#
# Each case runs the real scripts against a throwaway git repository holding only a copy of
# them, so nothing in this checkout is touched. The scripts ask npm which versions of the
# packages exist; a fake `npm`, first on PATH, answers from files in the directory named by
# FAKE_NPM instead, so no case reaches the network.
#
# The pack and publish scripts are run from this checkout: `verify` and publish only read the
# directory of tarballs they are given, and the cases for a whole pack run a copy of the pack
# script in a throwaway tree with a fake `pnpm`. The example script is run from this checkout
# with --no-install, against the real example and lockfile, and the smoke script against a fake
# `adb`. Nothing beyond bash, git, node and tar is needed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

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

# A fresh repository holding a copy of the scripts and a base commit on main, with origin/main
# at that commit as a fetch would leave it. Prints its path.
repo() {
    local dir
    dir="$(mktemp -d "$WORK/repo.XXXX")"
    mkdir -p "$dir/scripts"
    cp "$ROOT/scripts/android-sdk-version.sh" "$ROOT/scripts/android-sdk-changelog.sh" \
        "$ROOT/scripts/react-native-sdk-release.sh" "$dir/scripts/"
    git -C "$dir" init -q -b main
    git -C "$dir" config user.name test
    git -C "$dir" config user.email test@example.invalid
    git -C "$dir" config commit.gpgSign false
    git -C "$dir" config tag.gpgSign false
    git -C "$dir" add -A
    git -C "$dir" commit -q -m base
    git -C "$dir" update-ref refs/remotes/origin/main HEAD
    echo "$dir"
}

# Commits a change to $2 (a path inside the repository) with subject $3.
change() {
    local dir="$1" path="$2" subject="$3"
    mkdir -p "$(dirname "$dir/$path")"
    echo "$subject" >>"$dir/$path"
    git -C "$dir" add -A
    git -C "$dir" commit -q -m "$subject"
}

# Moves origin/main to HEAD, as fetching after a merge to main would.
sync_main() {
    git -C "$1" update-ref refs/remotes/origin/main HEAD
}

# Creates the annotated tag $2 with message $3 on HEAD.
annotate() {
    git -C "$1" tag -a -m "$3" "$2"
}

# The abbreviated hash of $2 in the repository $1.
short() {
    git -C "$1" rev-parse --short "${2:-HEAD}"
}

# A fresh, empty registry for the fake npm. Prints its path.
npm_dir() {
    mktemp -d "$WORK/npm.XXXX"
}

# npm_has <package> <json>: the fake registry lists <json> as the package's versions.
npm_has() {
    printf '%s\n' "$2" >"$FAKE_NPM/${1//\//__}.json"
}

# npm_fails <package> <output>: npm exits non-zero and prints <output>.
npm_fails() {
    printf '%s\n' "$2" >"$FAKE_NPM/${1//\//__}.error.json"
}

BINDINGS=@fedimint/react-native-bindings
WRAPPER=@fedimint/react-native

# The fake npm answers from files in $FAKE_NPM and records every call in its `calls` file:
#   npm --version                    $FAKE_NPM/npm-version, else 11.19.0
#   npm view <package> versions --json
#                                    the package's <name>.json (with `/` as `__`) when there is
#                                    one, else its <name>.error.json with a failing status, else
#                                    npm's own answer for a package that was never published
#   npm view <name>@<version> version --json
#                                    $FAKE_NPM/view-error.json with a failing status when it
#                                    exists, else the version if the registry holds it, else the
#                                    E404 answer, or nothing at all and success when
#                                    $FAKE_NPM/view-empty exists. The registry holds a version
#                                    when $FAKE_NPM/published/<name>@<version> exists (`/` as `__`).
#   npm view <name>@<version> dist.integrity
#                                    that marker's .integrity file
#   npm view <name> dist-tags --json
#                                    $FAKE_NPM/dist-tags-error.json with a failing status when it
#                                    exists, else the package's dist-tags/<name>.json (`/` as
#                                    `__`) when there is one, else the E404 answer
#   npm publish ...                  appends its arguments to $FAKE_NPM/log. A dry run fails when
#                                    $FAKE_NPM/dry-run-fails exists. Any other publish fails when
#                                    $FAKE_NPM/publish-fails exists, else it puts the marker for
#                                    the tarball's name and version in place, with the tarball's
#                                    integrity, sets the dist-tag given after --tag to the
#                                    tarball's version in the package's dist-tags/<name>.json, and
#                                    then fails anyway when $FAKE_NPM/publish-fails-but-lands
#                                    exists.
# With $FAKE_NPM/forbid-view, any `npm view` fails. Anything else fails loudly.
mkdir "$WORK/bin"
cat >"$WORK/bin/npm" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$FAKE_NPM/calls"
if [[ "${1:-}" == view && -f "$FAKE_NPM/forbid-view" ]]; then
    echo "npm view must not be called: $*" >&2
    exit 9
fi
if [[ $# -eq 1 && "$1" == --version ]]; then
    if [[ -f "$FAKE_NPM/npm-version" ]]; then
        cat "$FAKE_NPM/npm-version"
    else
        echo 11.19.0
    fi
    exit 0
fi
if [[ $# -eq 4 && "$1" == view && "$3" == versions && "$4" == --json ]]; then
    name="${2//\//__}"
    if [[ -f "$FAKE_NPM/$name.json" ]]; then
        cat "$FAKE_NPM/$name.json"
        exit 0
    fi
    if [[ -f "$FAKE_NPM/$name.error.json" ]]; then
        cat "$FAKE_NPM/$name.error.json"
        exit 1
    fi
    echo '{"error":{"code":"E404","summary":"Not Found"}}'
    exit 1
fi
if [[ $# -eq 4 && "${1:-}" == view && "$3" == dist-tags && "$4" == --json ]]; then
    name="${2//\//__}"
    if [[ -f "$FAKE_NPM/dist-tags-error.json" ]]; then
        cat "$FAKE_NPM/dist-tags-error.json"
        exit 1
    fi
    if [[ -f "$FAKE_NPM/dist-tags/$name.json" ]]; then
        cat "$FAKE_NPM/dist-tags/$name.json"
        exit 0
    fi
    echo '{"error":{"code":"E404","summary":"Not Found"}}'
    exit 1
fi
if [[ "${1:-}" == view && $# -eq 4 && "$3" == version && "$4" == --json ]] ||
    [[ "${1:-}" == view && $# -eq 3 && "$3" == dist.integrity ]]; then
    name="${2%@*}"
    version="${2##*@}"
    marker="$FAKE_NPM/published/${name//\//__}@$version"
    if [[ "$3" == version ]]; then
        if [[ -f "$FAKE_NPM/view-error.json" ]]; then
            cat "$FAKE_NPM/view-error.json"
            exit 1
        fi
        if [[ -f "$marker" ]]; then
            echo "\"$version\""
            exit 0
        fi
        [[ ! -f "$FAKE_NPM/view-empty" ]] || exit 0
        echo '{"error":{"code":"E404","summary":"Not Found"}}'
        exit 1
    fi
    cat "$marker.integrity"
    exit
fi
if [[ "${1:-}" == publish ]]; then
    echo "$*" >>"$FAKE_NPM/log"
    dry_run=0
    tarball=""
    tag=""
    previous=""
    for arg in "$@"; do
        [[ "$arg" != --dry-run ]] || dry_run=1
        [[ "$arg" != *.tgz ]] || tarball="$arg"
        [[ "$previous" != --tag ]] || tag="$arg"
        previous="$arg"
    done
    if ((dry_run)); then
        [[ ! -f "$FAKE_NPM/dry-run-fails" ]] || exit 1
        exit 0
    fi
    [[ ! -f "$FAKE_NPM/publish-fails" ]] || exit 1
    read -r package version < <(tar -xzOf "$tarball" package/package.json | node -e '
        const m = JSON.parse(require("fs").readFileSync(0, "utf8"));
        console.log(m.name + " " + m.version);
    ')
    identity="${package//\//__}@$version"
    mkdir -p "$FAKE_NPM/published" "$FAKE_NPM/dist-tags"
    : >"$FAKE_NPM/published/$identity"
    node -e '
        const crypto = require("crypto");
        const data = require("fs").readFileSync(process.argv[1]);
        console.log("sha512-" + crypto.createHash("sha512").update(data).digest("base64"));
    ' "$tarball" >"$FAKE_NPM/published/$identity.integrity"
    tags="$FAKE_NPM/dist-tags/${package//\//__}.json"
    node -e '
        const fs = require("fs");
        const [file, tag, version] = process.argv.slice(1);
        const tags = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : {};
        tags[tag] = version;
        fs.writeFileSync(file, JSON.stringify(tags) + "\n");
    ' "$tags" "$tag" "$version"
    [[ ! -f "$FAKE_NPM/publish-fails-but-lands" ]] || exit 1
    exit 0
fi
echo "unexpected npm call: $*" >&2
exit 9
EOF
chmod +x "$WORK/bin/npm"
export PATH="$WORK/bin:$PATH"
export FAKE_NPM
FAKE_NPM="$(npm_dir)"

# expect_ok <name> <expected stdout and stderr> -- <command...>
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

# expect_stdout <name> <expected stdout> -- <command...>: succeeds and prints exactly that on
# stdout, whatever it says on stderr.
expect_stdout() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>/dev/null)"; then
        if [[ "$out" == "$want" ]]; then
            pass "$name"
        else
            fail "$name" "expected: $want"$'\n'"got: $out"
        fi
    else
        fail "$name" "exited non-zero"
    fi
}

# expect_pass <name> <expected text in output> -- <command...>
expect_pass() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        if [[ "$out" == *"$want"* ]]; then
            pass "$name"
        else
            fail "$name" "expected the output to mention: $want"$'\n'"got: $out"
        fi
    else
        fail "$name" "exited non-zero: $out"
    fi
}

# expect_last <name> <expected last line> -- <command...>
expect_last() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        if [[ "${out##*$'\n'}" == "$want" ]]; then
            pass "$name"
        else
            fail "$name" "expected the last line: $want"$'\n'"got: $out"
        fi
    else
        fail "$name" "exited non-zero: $out"
    fi
}

# expect_fail <name> <expected text in stderr> -- <command...>
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

# expect_equal <name> <expected> <actual>
expect_equal() {
    if [[ "$3" == "$2" ]]; then
        pass "$1"
    else
        fail "$1" "expected: $2"$'\n'"got: $3"
    fi
}

# expect_contains <name> <expected text> <actual>
expect_contains() {
    if [[ "$3" == *"$2"* ]]; then
        pass "$1"
    else
        fail "$1" "expected the text to mention: $2"$'\n'"got: $3"
    fi
}

# run <command...>: runs the command and leaves its output (stdout and stderr) in $OUT and its
# exit status in $STATUS.
run() {
    STATUS=0
    OUT="$("$@" 2>&1)" || STATUS=$?
}

echo "react-native-sdk-release.sh"
echo "check"

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"

expect_pass "nothing on npm and no tags: any version passes" "0.1.0-beta.1" -- \
    "$rn" check 0.1.0-beta.1

FAKE_NPM="$(npm_dir)"
npm_has "$BINDINGS" '["0.0.0","0.0.0-canary-0123abc","0.0.0-om-rn1-4567def"]'
npm_has "$WRAPPER" '["0.0.0","0.0.0-canary-0123abc","0.0.0-om-rn1-4567def"]'
expect_pass "the placeholder and snapshot versions on npm do not block a beta" \
    "is newer than the latest release" -- "$rn" check 0.1.0-beta.1
expect_fail "0.0.0 is already on npm" "already released" -- "$rn" check 0.0.0
expect_fail "a beta of 0.0.0 is lower than 0.0.0 on npm" "is lower than" -- \
    "$rn" check 0.0.0-beta.1

FAKE_NPM="$(npm_dir)"
npm_has "$BINDINGS" '"0.0.0"'
expect_pass "a package with exactly one version is read (bare JSON string)" \
    "is newer than the latest release" -- "$rn" check 0.1.0-beta.1
expect_fail "its only version is taken" "already released" -- "$rn" check 0.0.0

FAKE_NPM="$(npm_dir)"
npm_has "$WRAPPER" '["0.2.0"]'
expect_fail "the second package is checked too" "$WRAPPER: 0.1.0 is lower than 0.2.0" -- \
    "$rn" check 0.1.0

FAKE_NPM="$(npm_dir)"
npm_fails "$WRAPPER" '{"error":{"code":"ECONNREFUSED","summary":"connect ECONNREFUSED"}}'
expect_fail "a registry error is not mistaken for no versions" "ECONNREFUSED" -- \
    "$rn" check 0.1.0-beta.1
expect_fail "the registry error names the package" "$WRAPPER" -- "$rn" check 0.1.0-beta.1

FAKE_NPM="$(npm_dir)"
npm_fails "$BINDINGS" 'npm is broken'
expect_fail "npm output that is not JSON is an error" "npm is broken" -- \
    "$rn" check 0.1.0-beta.1

FAKE_NPM="$(npm_dir)"
npm_has "$BINDINGS" 'not json'
expect_fail "a success that is not a JSON list is an error" "$BINDINGS" -- \
    "$rn" check 0.1.0-beta.1

FAKE_NPM="$(npm_dir)"
npm_has "$BINDINGS" '{"unexpected":true}'
expect_fail "a success that is neither a list nor a string is an error" "$BINDINGS" -- \
    "$rn" check 0.1.0-beta.1

FAKE_NPM="$(npm_dir)"
r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
expect_fail "a tagged version is already released" "already released" -- \
    "$rn" check 0.1.0-beta.1
expect_pass "the next beta passes" "tags: 0.1.0-beta.2 is newer than" -- \
    "$rn" check 0.1.0-beta.2

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.2.0 "Release."
expect_fail "a version below the latest tag is refused" "is lower than 0.2.0" -- \
    "$rn" check 0.1.0

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v1.0.0-rc.1 "Never released."
expect_pass "a tag outside the grammar blocks nothing" "tags: 0.2.0: nothing is released yet" -- \
    "$rn" check 0.2.0

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" android-sdk-v9.9.9 "Another SDK."
expect_pass "tags with another prefix are not read" "tags: 0.1.0-beta.1" -- \
    "$rn" check 0.1.0-beta.1

expect_fail "a version outside the grammar is refused" "must be X.Y.Z-beta.N or X.Y.Z" -- \
    "$rn" check 0.1.0-rc.1

echo
echo "check-tag"

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
expect_last "an annotated tag on main with a message can be released" \
    "react-native-sdk-v0.1.0-beta.1 can be released" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1

FAKE_NPM="$(npm_dir)"
npm_has "$BINDINGS" '["0.0.0","0.1.0-beta.1"]'
expect_last "a re-run passes when a package already has this very version" \
    "react-native-sdk-v0.1.0-beta.1 can be released" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1
FAKE_NPM="$(npm_dir)"
npm_has "$BINDINGS" '["0.0.0","0.1.0-beta.2"]'
expect_fail "a package holding a higher version still refuses the tag" "is lower than" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1
FAKE_NPM="$(npm_dir)"

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
git -C "$r" tag react-native-sdk-v0.1.0-beta.1
expect_fail "a lightweight tag is refused" "annotated" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
git -C "$r" checkout -q -b side
change "$r" side.txt "side work"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "On a side branch."
git -C "$r" checkout -q main
expect_fail "a tag on a commit that is not on origin/main is refused" "main" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1
expect_fail "the refusal names the tag and the commit" \
    "react-native-sdk-v0.1.0-beta.1 (commit $(short "$r" side))" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
change "$r" later.txt "merged to main, not fetched"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "Ahead of origin/main."
expect_fail "a commit ahead of a stale origin/main is refused" "main" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1
sync_main "$r"
expect_last "the same tag passes once origin/main has it" \
    "react-native-sdk-v0.1.0-beta.1 can be released" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1

expect_fail "another SDK's tag is refused" "is not a react-native-sdk-v* tag" -- \
    "$rn" check-tag android-sdk-v0.1.0
expect_fail "a tag that does not exist is refused" "no tag react-native-sdk-v0.9.9" -- \
    "$rn" check-tag react-native-sdk-v0.9.9

annotate "$r" react-native-sdk-v0.1.0-rc.1 "Outside the grammar."
expect_fail "an annotated tag outside the grammar is refused" \
    "must be X.Y.Z-beta.N or X.Y.Z" -- "$rn" check-tag react-native-sdk-v0.1.0-rc.1

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
annotate "$r" react-native-sdk-v0.1.0-beta.2 "Second beta."
expect_fail "a tag below a higher release is refused" "is lower than 0.1.0-beta.2" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1
expect_pass "the highest tag passes" "can be released" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.2

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
git -C "$r" tag -a -F - react-native-sdk-v0.1.0-beta.1 <<'EOF'
Release 0.1.0-beta.1

# Added

- Something.

Signed-off-by: A Maintainer <maintainer@example.invalid>
EOF
expect_pass "a multi-paragraph message with headings is fine" "can be released" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1

if git -C "$r" tag -a --cleanup=verbatim -m ' ' react-native-sdk-v0.1.0-beta.2 2>/dev/null; then
    expect_fail "a blank message is refused" "no release notes" -- \
        "$rn" check-tag react-native-sdk-v0.1.0-beta.2
else
    fail "a blank message is refused" "git refused to create a tag with a blank message"
fi
blank_lines=$'\n\n  \n'
if git -C "$r" tag -a --cleanup=verbatim -m "$blank_lines" react-native-sdk-v0.1.0-beta.3 \
    2>/dev/null; then
    expect_fail "a message of only blank lines is refused" "no release notes" -- \
        "$rn" check-tag react-native-sdk-v0.1.0-beta.3
else
    fail "a message of only blank lines is refused" "git refused to create the tag"
fi

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
git -C "$r" update-ref -d refs/remotes/origin/main
expect_fail "a checkout without origin/main is refused" "no origin/main" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.1

echo
echo "message"

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
# A first paragraph of several lines, a Markdown heading and a line starting with `#`: what
# reading the message as subject and body, or git's default cleanup, would each damage.
# The backticks are Markdown, not an expansion.
# shellcheck disable=SC2016
printf '%s\n' '- One fix.' '- Another fix,' '  on two lines.' '' '## Breaking' '' \
    '#42 changed `openSdk`.' >"$WORK/notes.md"
git -C "$r" tag -a --cleanup=whitespace -F "$WORK/notes.md" react-native-sdk-v0.1.0-beta.1
expect_stdout "message prints the notes as they were written" "$(cat "$WORK/notes.md")" -- \
    "$rn" message react-native-sdk-v0.1.0-beta.1
git -C "$r" tag react-native-sdk-v0.1.0-beta.2
expect_fail "message refuses a lightweight tag" "annotated" -- \
    "$rn" message react-native-sdk-v0.1.0-beta.2
expect_fail "message refuses another SDK's tag" "is not a react-native-sdk-v* tag" -- \
    "$rn" message android-sdk-v0.1.0
expect_fail "message refuses a tag that does not exist" "no tag react-native-sdk-v0.9.9" -- \
    "$rn" message react-native-sdk-v0.9.9
expect_usage "message without a tag" -- "$rn" message

echo
echo "notes"

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
status=0
out="$("$rn" notes 2>"$WORK/err")" || status=$?
if [[ "$status" -eq 0 && -z "$out" && "$(<"$WORK/err")" == *"nothing is released yet"* ]]; then
    pass "no earlier release: no notes on stdout, a note on stderr"
else
    fail "no earlier release: no notes on stdout, a note on stderr" \
        "status $status, stdout: $out"$'\n'"stderr: $(<"$WORK/err")"
fi

annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
change "$r" js/react-native/react-native/src/index.ts "feat(rn): change the API"
c1="$(short "$r")"
change "$r" rust/fedimint-sdk/src/lib.rs "fix(sdk): fix the core"
c2="$(short "$r")"
change "$r" flake.lock "chore(nix): bump the lock"
c3="$(short "$r")"
change "$r" scripts/generate-sdk-rn-bindings.sh "build(rn): change the generator"
c4="$(short "$r")"
change "$r" android/fedimint-sdk/x.kt "feat(android): not for React Native"
change "$r" js/web/index.ts "feat(web): not for React Native"
expect_stdout "notes list the commits that touch the packages, newest first" \
    "- build(rn): change the generator ($c4)
- chore(nix): bump the lock ($c3)
- fix(sdk): fix the core ($c2)
- feat(rn): change the API ($c1)" -- "$rn" notes
expect_stdout "notes up to a commit stop there" "- feat(rn): change the API ($c1)" -- \
    "$rn" notes "$c1"
expect_pass "notes say what they are on stderr" "since react-native-sdk-v0.1.0-beta.1" -- \
    "$rn" notes

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
change "$r" js/web/index.ts "feat(web): not for React Native"
expect_stdout "no commit touching the packages: a line saying so" \
    "- No changes to the React Native packages since 0.1.0-beta.1." -- "$rn" notes

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "First beta."
change "$r" js/react-native/react-native/src/old.ts "feat(rn): in the first beta"
annotate "$r" react-native-sdk-v0.1.0-beta.2 "Second beta."
change "$r" js/react-native/react-native/src/new.ts "feat(rn): after the second beta"
expect_stdout "the range starts at the latest release" \
    "- feat(rn): after the second beta ($(short "$r"))" -- "$rn" notes

echo
echo "tag"

FAKE_NPM="$(npm_dir)"
r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
printf 'Beta one.\n' >"$WORK/notes.md"
expect_pass "tag creates the tag" "react-native-sdk-v0.1.0-beta.1" -- \
    "$rn" tag 0.1.0-beta.1 "$WORK/notes.md"
expect_ok "the tag is annotated" "tag" -- \
    git -C "$r" cat-file -t refs/tags/react-native-sdk-v0.1.0-beta.1
expect_ok "the tag is on origin/main" "$(git -C "$r" rev-parse origin/main)" -- \
    git -C "$r" rev-parse "refs/tags/react-native-sdk-v0.1.0-beta.1^{commit}"
expect_ok "the tag message is the notes file" "Beta one." -- \
    git -C "$r" for-each-ref --format='%(contents)' refs/tags/react-native-sdk-v0.1.0-beta.1

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
printf '# Heading\n\nFixes:\n#123 fixed\n' >"$WORK/headings.md"
"$rn" tag 0.1.0-beta.1 "$WORK/headings.md" >/dev/null
message="$(git -C "$r" for-each-ref --format='%(contents)' \
    refs/tags/react-native-sdk-v0.1.0-beta.1)"
if [[ "$message" == *"# Heading"* && "$message" == *"#123 fixed"* ]]; then
    pass "lines starting with # survive in the tag message"
else
    fail "lines starting with # survive in the tag message" "got: $message"
fi

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
printf 'Relative notes.\n' >"$r/notes.md"
(cd "$r" && ./scripts/react-native-sdk-release.sh tag 0.1.0-beta.1 notes.md >/dev/null)
expect_ok "a notes file given by a relative path is read" "Relative notes." -- \
    git -C "$r" for-each-ref --format='%(contents)' refs/tags/react-native-sdk-v0.1.0-beta.1

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
change "$r" later.txt "a second commit"
sync_main "$r"
second="$(git -C "$r" rev-parse HEAD)"
change "$r" later.txt "a third commit, not on origin/main"
"$rn" tag 0.1.0-beta.2 "$WORK/notes.md" "$second" >/dev/null
expect_ok "a given commit is the one tagged" "$second" -- \
    git -C "$r" rev-parse "refs/tags/react-native-sdk-v0.1.0-beta.2^{commit}"
expect_fail "a commit that is not on origin/main is refused" "not on origin/main" -- \
    "$rn" tag 0.1.0-beta.3 "$WORK/notes.md" HEAD
expect_fail "a commit that does not exist is refused" "no commit nonexistent" -- \
    "$rn" tag 0.1.0-beta.3 "$WORK/notes.md" nonexistent
expect_fail "no tag was made for the refused commit" "no tag" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.3

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
annotate "$r" react-native-sdk-v0.1.0-beta.1 "Existing notes."
before="$(git -C "$r" rev-parse refs/tags/react-native-sdk-v0.1.0-beta.1)"
expect_fail "an existing tag is never replaced" "release tags are never moved" -- \
    "$rn" tag 0.1.0-beta.1 "$WORK/notes.md"
expect_ok "the existing tag is untouched" "$before" -- \
    git -C "$r" rev-parse refs/tags/react-native-sdk-v0.1.0-beta.1

expect_fail "a missing notes file is refused" "$WORK/missing.md" -- \
    "$rn" tag 0.1.0-beta.2 "$WORK/missing.md"
printf ' \n\n\t\n' >"$WORK/blank.md"
expect_fail "a notes file of only whitespace is refused" "no release notes" -- \
    "$rn" tag 0.1.0-beta.2 "$WORK/blank.md"
expect_fail "no tag was made for the missing or blank notes" "no tag" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.2

FAKE_NPM="$(npm_dir)"
npm_has "$WRAPPER" '["0.1.0-beta.2"]'
expect_fail "a version that fails the checks is refused" "already released" -- \
    "$rn" tag 0.1.0-beta.2 "$WORK/notes.md"
expect_fail "no tag was made for the refused version" "no tag" -- \
    "$rn" check-tag react-native-sdk-v0.1.0-beta.2
FAKE_NPM="$(npm_dir)"
expect_fail "a version outside the grammar is refused" "must be X.Y.Z-beta.N or X.Y.Z" -- \
    "$rn" tag 0.1.0-rc.1 "$WORK/notes.md"

expect_pass "tag prints the command that releases it" \
    "git push origin react-native-sdk-v0.1.0-beta.2" -- \
    "$rn" tag 0.1.0-beta.2 "$WORK/notes.md"

echo
echo "usage"

r="$(repo)"
rn="$r/scripts/react-native-sdk-release.sh"
expect_usage "no arguments" -- "$rn"
expect_usage "an unknown subcommand" -- "$rn" release
expect_usage "notes takes at most a commit" -- "$rn" notes HEAD HEAD
expect_usage "tag needs a version and a notes file" -- "$rn" tag 0.1.0-beta.1
expect_usage "tag takes at most a commit" -- "$rn" tag 0.1.0-beta.1 "$WORK/notes.md" HEAD HEAD
expect_usage "check needs a version" -- "$rn" check
expect_usage "check takes one version" -- "$rn" check 0.1.0-beta.1 0.1.0-beta.2
expect_usage "check-tag needs a tag" -- "$rn" check-tag
expect_usage "check-tag takes one tag" -- "$rn" check-tag react-native-sdk-v0.1.0 x

echo
echo "paths"

# The packages' source paths are read out of the script, so this checks the list the script uses.
paths=()
while IFS= read -r path; do
    paths+=("$path")
done < <(sed -n '/^PATHS=(/,/^)/p' "$ROOT/scripts/react-native-sdk-release.sh" |
    sed -e '1d' -e '$d' -e 's/^ *//')
if [[ ${#paths[@]} -eq 0 ]]; then
    fail "PATHS is read from the script" "found no entries"
fi
for path in ${paths[@]+"${paths[@]}"}; do
    if [[ -n "$(git -C "$ROOT" ls-files -- "$path")" ]]; then
        pass "PATHS entry $path exists in this checkout"
    else
        fail "PATHS entry $path exists in this checkout" "git ls-files lists nothing there"
    fi
done

echo
echo "react-native-sdk-pack.sh"

# The tarballs the pack and publish scripts handle are built here from small dummy files, with the
# entry names and manifests of a valid release. A case that needs a bad tarball starts from a
# valid one and breaks one thing.
VERSION=0.1.0-beta.1
BINDINGS_TGZ="fedimint-react-native-bindings-$VERSION.tgz"
WRAPPER_TGZ="fedimint-react-native-$VERSION.tgz"
REPO_URL=git+https://github.com/fedimint/fedimint-sdk.git

# The files the bindings package consists of, relative to its directory: the native libraries,
# the files that build the native module, the generated bindings and the files its manifest
# points at.
BINDINGS_FILES=(
    android/src/main/jniLibs/arm64-v8a/libfedimint_sdk.so
    android/src/main/jniLibs/x86_64/libfedimint_sdk.so
    FedimintReactNativeBindingsFramework.xcframework/Info.plist
    FedimintReactNativeBindingsFramework.xcframework/ios-arm64/libfedimint_sdk.a
    FedimintReactNativeBindingsFramework.xcframework/ios-arm64_x86_64-simulator/libfedimint_sdk.a
    ReactNativeBindings.podspec
    react-native.config.js
    android/build.gradle
    android/CMakeLists.txt
    android/generated/jni/CMakeLists.txt
    ios/ReactNativeBindings.mm
    cpp/generated/fedimint_sdk.cpp
    src/generated/fedimint_sdk.ts
    plugin/build/index.js
    app.plugin.js
    src/index.tsx
    lib/commonjs/index.js
    lib/module/index.js
    lib/typescript/module/src/index.d.ts
)
# The same for the wrapper package.
WRAPPER_FILES=(
    app.plugin.js
    README.md
    src/index.ts
    lib/commonjs/index.js
    lib/module/index.js
    lib/typescript/commonjs/index.d.ts
)

# manifest <bindings|wrapper> <version>: a manifest that passes every check of `verify`.
manifest() {
    if [[ "$1" == bindings ]]; then
        cat <<EOF
{
  "name": "$BINDINGS",
  "version": "$2",
  "source": "./src/index.tsx",
  "main": "./lib/commonjs/index.js",
  "module": "./lib/module/index.js",
  "types": "./lib/typescript/module/src/index.d.ts",
  "exports": {
    ".": {
      "source": "./src/index.tsx",
      "types": "./lib/typescript/module/src/index.d.ts",
      "default": "./lib/module/index.js"
    },
    "./app.plugin.js": "./app.plugin.js",
    "./package.json": "./package.json"
  },
  "scripts": {
    "prepare": "pnpm plugin:build"
  },
  "repository": {
    "type": "git",
    "url": "$REPO_URL",
    "directory": "js/react-native/react-native-bindings"
  },
  "publishConfig": {
    "access": "public"
  },
  "peerDependencies": {
    "react": ">=18.0.0"
  }
}
EOF
    else
        cat <<EOF
{
  "name": "$WRAPPER",
  "version": "$2",
  "source": "./src/index.ts",
  "main": "./lib/commonjs/index.js",
  "module": "./lib/module/index.js",
  "types": "./lib/typescript/commonjs/index.d.ts",
  "exports": {
    ".": {
      "source": "./src/index.ts",
      "types": "./lib/typescript/commonjs/index.d.ts",
      "default": "./lib/module/index.js"
    },
    "./app.plugin.js": "./app.plugin.js",
    "./package.json": "./package.json"
  },
  "scripts": {
    "build": "bob build"
  },
  "repository": {
    "type": "git",
    "url": "$REPO_URL",
    "directory": "js/react-native/react-native"
  },
  "publishConfig": {
    "access": "public"
  },
  "peerDependencies": {
    "$BINDINGS": "$2",
    "react": ">=18.0.0"
  }
}
EOF
    fi
}

# fill <bindings|wrapper> <version> <directory>: writes the package's dummy files and manifest.
fill() {
    local kind="$1" version="$2" dir="$3" file
    local -a files
    if [[ "$kind" == bindings ]]; then
        files=("${BINDINGS_FILES[@]}")
    else
        files=("${WRAPPER_FILES[@]}")
    fi
    for file in "${files[@]}"; do
        mkdir -p "$dir/$(dirname "$file")"
        echo "$file" >"$dir/$file"
    done
    manifest "$kind" "$version" >"$dir/package.json"
}

# tarballs <dir> <version>: a valid pair of tarballs for <version> in <dir>.
tarballs() {
    local dir="$1" version="$2" stage
    mkdir -p "$dir"
    stage="$(mktemp -d "$WORK/stage.XXXX")"
    mkdir "$stage/package"
    fill bindings "$version" "$stage/package"
    tar -czf "$dir/fedimint-react-native-bindings-$version.tgz" -C "$stage" package
    rm -rf "${stage:?}/package"
    mkdir "$stage/package"
    fill native "$version" "$stage/package"
    tar -czf "$dir/fedimint-react-native-$version.tgz" -C "$stage" package
}

# rewrite <tarball> <command...>: unpacks the tarball, runs the command with the unpacked
# `package` directory as its last argument, and packs the result in the tarball's place.
rewrite() {
    local tarball="$1" tmp
    shift
    tmp="$(mktemp -d "$WORK/rewrite.XXXX")"
    tar -xzf "$tarball" -C "$tmp"
    "$@" "$tmp/package"
    tar -czf "$tarball" -C "$tmp" package
}

# drop <path> <package dir>
drop() {
    rm "${2:?}/$1"
}

# add <path> <package dir>
add() {
    mkdir -p "$(dirname "$2/$1")"
    echo added >"$2/$1"
}

# edit_manifest <javascript> <package dir>: runs the code on the package's manifest, `m`.
edit_manifest() {
    node -e '
        const fs = require("fs");
        const m = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
        new Function("m", process.argv[1])(m);
        fs.writeFileSync(process.argv[2], JSON.stringify(m, null, 2) + "\n");
    ' "$1" "$2/package.json"
}

# A fresh directory holding a valid pair of tarballs. Prints its path.
valid_pair() {
    local dir
    dir="$(mktemp -d "$WORK/tgz.XXXX")"
    tarballs "$dir" "$VERSION"
    echo "$dir"
}

# integrity <file>: the `sha512-<base64>` digest npm calls a file's integrity.
integrity() {
    node -e '
        const crypto = require("crypto");
        const data = require("fs").readFileSync(process.argv[1]);
        console.log("sha512-" + crypto.createHash("sha512").update(data).digest("base64"));
    ' "$1"
}

# sha256 <file>: the file's SHA-256 as hexadecimal digits.
sha256() {
    node -e '
        const crypto = require("crypto");
        const data = require("fs").readFileSync(process.argv[1]);
        console.log(crypto.createHash("sha256").update(data).digest("hex"));
    ' "$1"
}

# same <file> <file>: prints `same` or `different`.
same() {
    if cmp -s "$1" "$2"; then
        echo same
    else
        echo different
    fi
}

pack="$ROOT/scripts/react-native-sdk-pack.sh"

echo "verify"

d="$(valid_pair)"
expect_ok "a valid pair is fit to publish" \
    "$BINDINGS_TGZ: fit to publish"$'\n'"$WRAPPER_TGZ: fit to publish" -- \
    "$pack" verify "$VERSION" "$d"

d="$(valid_pair)"
rewrite "$d/$BINDINGS_TGZ" edit_manifest \
    'm.repository.url = "https://github.com/fedimint/fedimint-sdk"'
rewrite "$d/$WRAPPER_TGZ" edit_manifest \
    'm.repository.url = "https://github.com/fedimint/fedimint-sdk.git"'
expect_ok "a repository URL with only one of the git+ prefix and .git suffix is accepted" \
    "$BINDINGS_TGZ: fit to publish"$'\n'"$WRAPPER_TGZ: fit to publish" -- \
    "$pack" verify "$VERSION" "$d"

# break_manifest <name> <expected text> <javascript> [<tarball name>]: a valid pair whose manifest
# `m` was changed by the code in one tarball (the bindings by default).
break_manifest() {
    local name="$1" want="$2" code="$3" tarball="${4:-$BINDINGS_TGZ}"
    d="$(valid_pair)"
    rewrite "$d/$tarball" edit_manifest "$code"
    expect_fail "$name" "$tarball: $want" -- "$pack" verify "$VERSION" "$d"
}

# break_entries <name> <expected text> <tarball name> <command...>: a valid pair whose tarball was
# rewritten by the command.
break_entries() {
    local name="$1" want="$2" tarball="$3"
    shift 3
    d="$(valid_pair)"
    rewrite "$d/$tarball" "$@"
    expect_fail "$name" "$tarball: $want" -- "$pack" verify "$VERSION" "$d"
}

break_manifest "private is refused" 'has a "private" field' 'm.private = true'
break_manifest "private is refused in the wrapper" 'has a "private" field' 'm.private = false' \
    "$WRAPPER_TGZ"
break_manifest "a different version is refused" \
    'version is "0.1.0-beta.2", expected "0.1.0-beta.1"' 'm.version = "0.1.0-beta.2"'
break_manifest "a different name is refused" "name is \"$WRAPPER\", expected \"$BINDINGS\"" \
    "m.name = \"$WRAPPER\""
break_manifest "a workspace range left in peerDependencies is refused" \
    'contains "workspace:" ranges' \
    'm.peerDependencies["@fedimint/other"] = "workspace:*"'
break_manifest "a workspace range left in resolutions is refused" 'contains "workspace:" ranges' \
    'm.resolutions = { "@fedimint/other": "workspace:*" }'
break_manifest "a workspace range left in the wrapper is refused" 'contains "workspace:" ranges' \
    'm.devDependencies = { "@fedimint/other": "workspace:^" }' "$WRAPPER_TGZ"
break_manifest "a postinstall script is refused" 'scripts.postinstall is set' \
    'm.scripts.postinstall = "node download.js"'
break_manifest "a preinstall script is refused" 'scripts.preinstall is set' \
    'm.scripts.preinstall = "node download.js"'
break_manifest "an install script is refused" 'scripts.install is set' \
    'm.scripts.install = "node download.js"' "$WRAPPER_TGZ"
break_manifest "a missing publishConfig is refused" 'publishConfig.access is not "public"' \
    'delete m.publishConfig'
break_manifest "a restricted access is refused" 'publishConfig.access is not "public"' \
    'm.publishConfig.access = "restricted"' "$WRAPPER_TGZ"
break_manifest "another repository is refused" \
    'repository.url is "https://github.com/someone/else.git", expected' \
    'm.repository.url = "https://github.com/someone/else.git"'
break_manifest "a missing repository is refused" 'repository.url is missing' \
    'delete m.repository'
break_manifest "a main that is not in the tarball is refused" \
    'main points at ./lib/missing.js, which is not in the tarball' \
    'm.main = "./lib/missing.js"'
break_manifest "a module that is not in the tarball is refused" \
    'module points at ./lib/missing.js, which is not in the tarball' \
    'm.module = "./lib/missing.js"' "$WRAPPER_TGZ"
break_manifest "types that are not in the tarball are refused" \
    'types points at ./lib/missing.d.ts, which is not in the tarball' \
    'm.types = "./lib/missing.d.ts"'
break_manifest "a source that is not in the tarball is refused" \
    'source points at ./src/missing.ts, which is not in the tarball' \
    'm.source = "./src/missing.ts"'
break_manifest "an exports target that is not in the tarball is refused" \
    'exports["./extra"] points at ./lib/missing.js, which is not in the tarball' \
    'm.exports["./extra"] = "./lib/missing.js"'
break_manifest "a nested exports target that is not in the tarball is refused" \
    'exports["."]["import"]["default"] points at ./lib/missing.js, which is not in the tarball' \
    'm.exports["."].import = { default: "./lib/missing.js" }'

for entry in \
    android/src/main/jniLibs/arm64-v8a/libfedimint_sdk.so \
    android/src/main/jniLibs/x86_64/libfedimint_sdk.so \
    FedimintReactNativeBindingsFramework.xcframework/Info.plist \
    FedimintReactNativeBindingsFramework.xcframework/ios-arm64/libfedimint_sdk.a \
    FedimintReactNativeBindingsFramework.xcframework/ios-arm64_x86_64-simulator/libfedimint_sdk.a \
    ReactNativeBindings.podspec react-native.config.js android/build.gradle \
    android/CMakeLists.txt android/generated/jni/CMakeLists.txt ios/ReactNativeBindings.mm \
    cpp/generated/fedimint_sdk.cpp src/generated/fedimint_sdk.ts plugin/build/index.js \
    app.plugin.js; do
    break_entries "the bindings without $entry are refused" "missing entry package/$entry" \
        "$BINDINGS_TGZ" drop "$entry"
done
break_entries "the wrapper without app.plugin.js is refused" \
    "missing entry package/app.plugin.js" "$WRAPPER_TGZ" drop app.plugin.js
break_entries "the wrapper without a README is refused" \
    "missing entry package/README.md" "$WRAPPER_TGZ" drop README.md
break_entries "a libc++_shared.so in the bindings is refused" \
    "contains package/android/src/main/jniLibs/arm64-v8a/libc++_shared.so" "$BINDINGS_TGZ" \
    add android/src/main/jniLibs/arm64-v8a/libc++_shared.so
break_entries "a libc++_shared.so anywhere in the bindings is refused" \
    "contains package/lib/libc++_shared.so" "$BINDINGS_TGZ" add lib/libc++_shared.so

break_manifest "a bindings peer with a range is refused" \
    "peerDependencies[\"$BINDINGS\"] is \"^$VERSION\", expected exactly \"$VERSION\"" \
    "m.peerDependencies[\"$BINDINGS\"] = \"^$VERSION\"" "$WRAPPER_TGZ"
break_manifest "a bindings peer of another version is refused" \
    "peerDependencies[\"$BINDINGS\"] is \"0.1.0-beta.2\", expected exactly \"$VERSION\"" \
    "m.peerDependencies[\"$BINDINGS\"] = \"0.1.0-beta.2\"" "$WRAPPER_TGZ"
break_manifest "an absent bindings peer is refused" \
    "peerDependencies[\"$BINDINGS\"] is missing, expected exactly \"$VERSION\"" \
    "delete m.peerDependencies[\"$BINDINGS\"]" "$WRAPPER_TGZ"

d="$(valid_pair)"
rm "${d:?}/$WRAPPER_TGZ"
expect_fail "a missing wrapper tarball is refused" "$WRAPPER_TGZ: not found in $d" -- \
    "$pack" verify "$VERSION" "$d"
expect_fail "a missing wrapper tarball does not hide the bindings' verdict" \
    "$BINDINGS_TGZ: fit to publish" -- "$pack" verify "$VERSION" "$d"
d="$(valid_pair)"
rm "${d:?}/$BINDINGS_TGZ"
expect_fail "a missing bindings tarball is refused" "$BINDINGS_TGZ: not found in $d" -- \
    "$pack" verify "$VERSION" "$d"
expect_fail "a directory that does not exist is refused" "$WORK/nothing" -- \
    "$pack" verify "$VERSION" "$WORK/nothing"

d="$(valid_pair)"
rewrite "$d/$BINDINGS_TGZ" edit_manifest 'm.private = true'
rewrite "$d/$WRAPPER_TGZ" drop README.md
expect_fail "a problem in the bindings is reported next to one in the wrapper (bindings)" \
    "$BINDINGS_TGZ: has a \"private\" field" -- "$pack" verify "$VERSION" "$d"
expect_fail "a problem in the bindings is reported next to one in the wrapper (wrapper)" \
    "$WRAPPER_TGZ: missing entry package/README.md" -- "$pack" verify "$VERSION" "$d"
d="$(valid_pair)"
rewrite "$d/$BINDINGS_TGZ" edit_manifest 'm.private = true; m.version = "9.9.9"'
expect_fail "two problems in one tarball are both reported (private)" \
    "$BINDINGS_TGZ: has a \"private\" field" -- "$pack" verify "$VERSION" "$d"
expect_fail "two problems in one tarball are both reported (version)" \
    "$BINDINGS_TGZ: version is \"9.9.9\"" -- "$pack" verify "$VERSION" "$d"

d="$(valid_pair)"
expect_fail "a version other than the one in the file names is refused" \
    "fedimint-react-native-bindings-0.2.0.tgz: not found in $d" -- \
    "$pack" verify 0.2.0 "$d"
cp "$d/$BINDINGS_TGZ" "$d/fedimint-react-native-bindings-0.2.0.tgz"
cp "$d/$WRAPPER_TGZ" "$d/fedimint-react-native-0.2.0.tgz"
expect_fail "a tarball whose manifest differs from its file name is refused" \
    'version is "0.1.0-beta.1", expected "0.2.0"' -- "$pack" verify 0.2.0 "$d"

echo "pack"

cd "$ROOT"
manifests_before="$(cksum js/react-native/react-native/package.json \
    js/react-native/react-native-bindings/package.json)"
expect_fail "a version outside the grammar is refused" "must be X.Y.Z-beta.N or X.Y.Z" -- \
    "$pack" 0.1.0-rc.1 "$WORK/never/tarballs"
expect_equal "a refused version leaves no output directory" absent \
    "$([[ -e "$WORK/never" ]] && echo present || echo absent)"
expect_equal "a refused version leaves the manifests as they were" "$manifests_before" \
    "$(cksum js/react-native/react-native/package.json \
        js/react-native/react-native-bindings/package.json)"

expect_usage "no arguments" -- "$pack"
expect_usage "a version without an output directory" -- "$pack" 0.1.0-beta.1
expect_usage "three arguments" -- "$pack" 0.1.0-beta.1 "$WORK/out" extra
expect_usage "verify without a directory" -- "$pack" verify 0.1.0-beta.1
expect_usage "verify with four arguments" -- "$pack" verify 0.1.0-beta.1 "$WORK/out" extra

# A throwaway tree laid out like this repository, holding a copy of the pack script, everything
# a pack needs on disk, and the manifests as they are in the tree: private, at 0.0.0, with a
# workspace range for the bindings. Prints its path.
pack_tree() {
    local dir bindings native
    dir="$(mktemp -d "$WORK/tree.XXXX")"
    bindings="$dir/js/react-native/react-native-bindings"
    native="$dir/js/react-native/react-native"
    mkdir -p "$dir/scripts" "$dir/js/node_modules"
    cp "$ROOT/scripts/android-sdk-version.sh" "$ROOT/scripts/react-native-sdk-pack.sh" \
        "$dir/scripts/"
    fill bindings 0.0.0 "$bindings"
    fill native 0.0.0 "$native"
    edit_manifest 'm.private = true' "$bindings"
    edit_manifest "m.private = true; m.peerDependencies[\"$BINDINGS\"] = \"workspace:*\"" "$native"
    echo "$dir"
}

# A pnpm that packs a package directory the way `pnpm pack` does for the cases below: the
# directory as it is, with the workspace range replaced by the package's own version. It fails
# when FAKE_PNPM_FAIL is set, and anything but a pack fails loudly.
mkdir "$WORK/pnpm-bin"
cat >"$WORK/pnpm-bin/pnpm" <<'EOF'
#!/usr/bin/env bash
if [[ $# -ne 5 || "$1" != --dir || "$3" != pack || "$4" != --pack-destination ]]; then
    echo "unexpected pnpm call: $*" >&2
    exit 9
fi
[[ -z "${FAKE_PNPM_FAIL:-}" ]] || exit 1
stage="$(mktemp -d)"
mkdir "$stage/package"
cp -R "$2/." "$stage/package/"
identity="$(node -e '
    const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
    console.log(m.name.replace("@", "").replace("/", "-") + "-" + m.version + ".tgz", m.version);
' "$stage/package/package.json")"
read -r tarball version <<<"$identity"
sed "s/workspace:\*/$version/g" "$stage/package/package.json" >"$stage/package.json.new"
mv "$stage/package.json.new" "$stage/package/package.json"
tar -czf "$5/$tarball" -C "$stage" package
rm -rf "${stage:?}"
EOF
chmod +x "$WORK/pnpm-bin/pnpm"

tree="$(pack_tree)"
tree_bindings="$tree/js/react-native/react-native-bindings"
tree_native="$tree/js/react-native/react-native"
tree_pack="$tree/scripts/react-native-sdk-pack.sh"
cp "$tree_bindings/package.json" "$WORK/bindings-package.json"
cp "$tree_native/package.json" "$WORK/native-package.json"
out="$WORK/packed/tarballs"
mkdir -p "$out"
echo old >"$out/fedimint-react-native-0.0.9.tgz"
echo old >"$out/fedimint-react-native-bindings-0.0.9.tgz"
echo keep >"$out/notes.txt"
run env PATH="$WORK/pnpm-bin:$PATH" "$tree_pack" "$VERSION" "$out"
expect_equal "a pack of a complete tree succeeds" 0 "$STATUS"
expect_contains "the pack runs the checks on its tarballs" \
    "$BINDINGS_TGZ: fit to publish"$'\n'"$WRAPPER_TGZ: fit to publish" "$OUT"
expect_contains "the report names the bindings tarball" "$BINDINGS_TGZ" "$OUT"
expect_contains "the report names the wrapper tarball" "$WRAPPER_TGZ" "$OUT"
expect_contains "the report has the bindings' SHA-256" "$(sha256 "$out/$BINDINGS_TGZ")" "$OUT"
expect_contains "the report has the wrapper's SHA-256" "$(sha256 "$out/$WRAPPER_TGZ")" "$OUT"
expect_contains "the report has sizes in MB" " MB" "$OUT"
expect_contains "the report lists the Android library of arm64" \
    "package/android/src/main/jniLibs/arm64-v8a/libfedimint_sdk.so" "$OUT"
expect_contains "the report lists a static iOS library" \
    "package/FedimintReactNativeBindingsFramework.xcframework/ios-arm64/libfedimint_sdk.a" "$OUT"
expect_equal "earlier tarballs are removed from the output directory" "" \
    "$(ls "$out/fedimint-react-native-0.0.9.tgz" "$out/fedimint-react-native-bindings-0.0.9.tgz" \
        2>/dev/null || true)"
expect_equal "other files in the output directory are kept" keep "$(cat "$out/notes.txt")"
expect_equal "the bindings manifest is restored" same \
    "$(same "$tree_bindings/package.json" "$WORK/bindings-package.json")"
expect_equal "the wrapper manifest is restored" same \
    "$(same "$tree_native/package.json" "$WORK/native-package.json")"
expect_ok "the tarballs it wrote pass verify" \
    "$BINDINGS_TGZ: fit to publish"$'\n'"$WRAPPER_TGZ: fit to publish" -- \
    "$pack" verify "$VERSION" "$out"

add android/src/main/jniLibs/arm64-v8a/libc++_shared.so "$tree_bindings"
run env PATH="$WORK/pnpm-bin:$PATH" "$tree_pack" "$VERSION" "$WORK/packed/second"
expect_equal "a pack whose tarballs fail the checks fails" 1 "$STATUS"
expect_contains "it says why" "libc++_shared.so" "$OUT"
expect_equal "the bindings manifest is restored after a failed check" same \
    "$(same "$tree_bindings/package.json" "$WORK/bindings-package.json")"
expect_equal "the wrapper manifest is restored after a failed check" same \
    "$(same "$tree_native/package.json" "$WORK/native-package.json")"

run env PATH="$WORK/pnpm-bin:$PATH" FAKE_PNPM_FAIL=1 "$tree_pack" "$VERSION" "$WORK/packed/third"
expect_equal "a pack whose pnpm fails fails" 1 "$STATUS"
expect_equal "the bindings manifest is restored after a failed pnpm" same \
    "$(same "$tree_bindings/package.json" "$WORK/bindings-package.json")"
expect_equal "the wrapper manifest is restored after a failed pnpm" same \
    "$(same "$tree_native/package.json" "$WORK/native-package.json")"

tree="$(pack_tree)"
tree_bindings="$tree/js/react-native/react-native-bindings"
tree_native="$tree/js/react-native/react-native"
tree_pack="$tree/scripts/react-native-sdk-pack.sh"
cp "$tree_bindings/package.json" "$WORK/bindings-package.json"
rm -rf "${tree:?}/js/node_modules"
rm "${tree_bindings:?}/android/src/main/jniLibs/x86_64/libfedimint_sdk.so"
rm "${tree_bindings:?}/FedimintReactNativeBindingsFramework.xcframework/Info.plist"
rm "${tree_bindings:?}/plugin/build/index.js"
rm "${tree_native:?}/lib/module/index.js"
run env PATH="$WORK/pnpm-bin:$PATH" "$tree_pack" "$VERSION" "$WORK/packed/fourth"
expect_equal "a tree that is not built is refused" 1 "$STATUS"
expect_contains "the missing install is named" \
    "js/node_modules: run pnpm --dir js install" "$OUT"
expect_contains "a missing build output of the bindings is named" \
    "react-native-bindings/plugin/build/index.js: run pnpm --dir js run build:reactnative" "$OUT"
expect_contains "a missing build output of the wrapper is named" \
    "react-native/lib/module/index.js: run pnpm --dir js run build:reactnative" "$OUT"
expect_contains "a missing Android library is named" \
    "x86_64/libfedimint_sdk.so: run scripts/generate-sdk-rn-bindings.sh" "$OUT"
expect_contains "a missing xcframework file is named" \
    "Info.plist: run scripts/assemble-rn-ios-xcframework.sh" "$OUT"
expect_equal "a tree that is not built gets no output directory" absent \
    "$([[ -e "$WORK/packed/fourth" ]] && echo present || echo absent)"
expect_equal "a tree that is not built keeps its manifests" same \
    "$(same "$tree_bindings/package.json" "$WORK/bindings-package.json")"

echo
echo "react-native-sdk-publish.sh"

publish="$ROOT/scripts/react-native-sdk-publish.sh"

# on_registry <name> <version> <file>: the fake npm holds <name>@<version>, with <file> as its
# tarball.
on_registry() {
    local name="$1" version="$2" file="$3" marker
    mkdir -p "$FAKE_NPM/published"
    marker="$FAKE_NPM/published/${name//\//__}@$version"
    : >"$marker"
    integrity "$file" >"$marker.integrity"
}

# registry_tags <name> <json>: the fake npm reports <json> as the dist-tags of <name>.
registry_tags() {
    mkdir -p "$FAKE_NPM/dist-tags"
    printf '%s\n' "$2" >"$FAKE_NPM/dist-tags/${1//\//__}.json"
}

# sums_state <dir>: prints `present` or `absent` for <dir>/SHA256SUMS.
sums_state() {
    if [[ -e "$1/SHA256SUMS" ]]; then
        echo present
    else
        echo absent
    fi
}

# registry_integrity <name> <version> <digest>: the fake npm reports <digest> as the integrity of
# what it holds for <name>@<version>.
registry_integrity() {
    echo "$3" >"$FAKE_NPM/published/${1//\//__}@$2.integrity"
}

# publish_log <dir>: the publish calls the fake npm saw, without the directory of the tarballs.
publish_log() {
    sed "s#$1/##g" "$FAKE_NPM/log" 2>/dev/null || true
}

# check_sums <dir>: whether the system's own checker accepts <dir>/SHA256SUMS.
check_sums() {
    if command -v sha256sum >/dev/null; then
        (cd "$1" && sha256sum -c SHA256SUMS >/dev/null 2>&1) && echo ok || echo bad
    else
        (cd "$1" && shasum -a 256 -c SHA256SUMS >/dev/null 2>&1) && echo ok || echo bad
    fi
}

# fresh <version>: an empty fake registry and a directory with a valid pair of tarballs for the
# version, in $d.
fresh() {
    FAKE_NPM="$(npm_dir)"
    d="$(mktemp -d "$WORK/pub.XXXX")"
    tarballs "$d" "$1"
}

BT="fedimint-react-native-bindings-$VERSION.tgz"
NT="fedimint-react-native-$VERSION.tgz"

fresh "$VERSION"
run "$publish" "$d"
expect_equal "a release of a beta that is nowhere on npm succeeds" 0 "$STATUS"
expect_contains "the bindings are published first, under beta" \
    "published $BINDINGS@$VERSION under beta"$'\n'"published $WRAPPER@$VERSION under beta" "$OUT"
expect_equal "npm publish is called for the bindings first, then the wrapper" \
    "publish $BT --tag beta --access public"$'\n'"publish $NT --tag beta --access public" \
    "$(publish_log "$d")"
expect_equal "SHA256SUMS lists both tarballs, bindings first, with their digests" \
    "$(sha256 "$d/$BT")  $BT"$'\n'"$(sha256 "$d/$NT")  $NT" "$(cat "$d/SHA256SUMS")"
expect_equal "SHA256SUMS is accepted by the checker" ok "$(check_sums "$d")"
run "$publish" "$d"
expect_equal "running the release again succeeds" 0 "$STATUS"
expect_contains "the second run skips both packages" \
    "$BINDINGS@$VERSION is already on npm, skipped"$'\n'\
"$WRAPPER@$VERSION is already on npm, skipped" \
    "$OUT"
expect_equal "the second run publishes nothing" 2 "$(wc -l <"$FAKE_NPM/log" | tr -d ' ')"
expect_equal "SHA256SUMS is accepted by the checker after the second run" ok "$(check_sums "$d")"

BT0=fedimint-react-native-bindings-0.1.0.tgz
NT0=fedimint-react-native-0.1.0.tgz
fresh 0.1.0
run "$publish" "$d"
expect_equal "a release of a final version succeeds" 0 "$STATUS"
expect_equal "both packages are published under latest" \
    "publish $BT0 --tag latest --access public"$'\n'"publish $NT0 --tag latest --access public" \
    "$(publish_log "$d")"

fresh "$VERSION"
on_registry "$BINDINGS" "$VERSION" "$d/$BT"
cp "$d/$BT" "$WORK/bindings-before.tgz"
run "$publish" "$d"
expect_equal "bindings that are already on npm: the release succeeds" 0 "$STATUS"
expect_contains "the bindings are skipped" "$BINDINGS@$VERSION is already on npm, skipped" "$OUT"
expect_contains "the wrapper is published" "published $WRAPPER@$VERSION under beta" "$OUT"
expect_equal "only the wrapper is published" "publish $NT --tag beta --access public" \
    "$(publish_log "$d")"
expect_equal "the local bindings tarball is unchanged" same \
    "$(same "$d/$BT" "$WORK/bindings-before.tgz")"

fresh "$VERSION"
on_registry "$BINDINGS" "$VERSION" "$d/$BT"
on_registry "$WRAPPER" "$VERSION" "$d/$NT"
run "$publish" "$d"
expect_equal "both packages already on npm: the release succeeds" 0 "$STATUS"
expect_equal "nothing is published" "" "$(publish_log "$d")"
expect_equal "SHA256SUMS is still written" "$(sha256 "$d/$BT")  $BT"$'\n'"$(sha256 "$d/$NT")  $NT" \
    "$(cat "$d/SHA256SUMS")"

fresh "$VERSION"
served="$WORK/served.tgz"
echo "what npm serves" >"$served"
on_registry "$BINDINGS" "$VERSION" "$served"
cp "$d/$BT" "$WORK/bindings-before.tgz"
run "$publish" "$d"
expect_equal "bindings on npm with another digest: the release fails" 1 "$STATUS"
expect_contains "the refusal names the package and says it is another tarball" \
    "$BINDINGS@$VERSION is on npm as another tarball" "$OUT"
expect_equal "nothing is published next to bindings that are another tarball" "" \
    "$(publish_log "$d")"
expect_equal "the local bindings tarball is unchanged" same \
    "$(same "$d/$BT" "$WORK/bindings-before.tgz")"
expect_equal "no SHA256SUMS is written for bindings that are another tarball" absent \
    "$(sums_state "$d")"

fresh "$VERSION"
on_registry "$BINDINGS" "$VERSION" "$d/$BT"
registry_integrity "$BINDINGS" "$VERSION" "md5-AAAA"
cp "$d/$BT" "$WORK/bindings-before.tgz"
expect_fail "a dist.integrity that is not a sha512 digest is refused" "not a sha512 digest" -- \
    "$publish" "$d"
expect_equal "the local bindings tarball is kept when dist.integrity is refused" same \
    "$(same "$d/$BT" "$WORK/bindings-before.tgz")"
expect_equal "nothing is published when dist.integrity is refused" "" "$(publish_log "$d")"

fresh "$VERSION"
echo '{"error":{"code":"ECONNREFUSED"}}' >"$FAKE_NPM/view-error.json"
expect_fail "a registry that cannot be reached is refused" "ECONNREFUSED" -- "$publish" "$d"
expect_equal "nothing is published when the registry cannot be reached" "" "$(publish_log "$d")"
expect_equal "no SHA256SUMS is written when the registry cannot be reached" absent \
    "$([[ -e "$d/SHA256SUMS" ]] && echo present || echo absent)"

fresh "$VERSION"
: >"$FAKE_NPM/publish-fails"
expect_fail "a failed publish fails the release" "$BINDINGS@$VERSION" -- "$publish" "$d"
expect_equal "the wrapper is not attempted after the bindings failed" \
    "publish $BT --tag beta --access public" "$(publish_log "$d")"
expect_equal "no SHA256SUMS is written after a failed publish" absent \
    "$([[ -e "$d/SHA256SUMS" ]] && echo present || echo absent)"

fresh "$VERSION"
: >"$FAKE_NPM/publish-fails-but-lands"
run "$publish" "$d"
expect_equal "a publish that fails but lands is carried on from" 0 "$STATUS"
expect_contains "the bindings are reported as on npm despite the failure" \
    "$BINDINGS@$VERSION is on npm although npm publish reported a failure" "$OUT"
expect_contains "the wrapper is reported as on npm despite the failure" \
    "$WRAPPER@$VERSION is on npm although npm publish reported a failure" "$OUT"
expect_equal "both packages are on the registry" "yes yes" \
    "$([[ -e "$FAKE_NPM/published/${BINDINGS//\//__}@$VERSION" ]] && echo -n yes || echo -n no) \
$([[ -e "$FAKE_NPM/published/${WRAPPER//\//__}@$VERSION" ]] && echo yes || echo no)"

fresh "$VERSION"
echo 10.9.9 >"$FAKE_NPM/npm-version"
expect_fail "an npm older than 11.5.1 is refused" "11.5.1" -- "$publish" "$d"
expect_fail "the npm that was found is named" "10.9.9" -- "$publish" "$d"
expect_equal "an old npm is refused before any other npm call" "--version" \
    "$(sort -u "$FAKE_NPM/calls")"
run "$publish" --dry-run "$d"
expect_equal "an old npm is fine for a dry run" 0 "$STATUS"
expect_equal "an old npm publishes both tarballs in a dry run" 2 \
    "$(wc -l <"$FAKE_NPM/log" | tr -d ' ')"

fresh "$VERSION"
: >"$FAKE_NPM/forbid-view"
run "$publish" --dry-run "$d"
expect_equal "a dry run succeeds without asking the registry" 0 "$STATUS"
expect_equal "a dry run runs npm publish --dry-run twice, bindings first" \
    "publish --dry-run $BT --tag beta --access public"$'\n'\
"publish --dry-run $NT --tag beta --access public" \
    "$(publish_log "$d")"
expect_equal "a dry run writes no SHA256SUMS" absent \
    "$([[ -e "$d/SHA256SUMS" ]] && echo present || echo absent)"
expect_equal "a dry run publishes nothing" absent \
    "$([[ -e "$FAKE_NPM/published" ]] && echo present || echo absent)"
fresh 0.1.0
: >"$FAKE_NPM/forbid-view"
run "$publish" --dry-run "$d"
expect_equal "a dry run of a final version uses latest" \
    "publish --dry-run $BT0 --tag latest --access public"$'\n'\
"publish --dry-run $NT0 --tag latest --access public" \
    "$(publish_log "$d")"
fresh "$VERSION"
: >"$FAKE_NPM/dry-run-fails"
expect_fail "a dry run that fails fails the script" "$BINDINGS@$VERSION" -- \
    "$publish" --dry-run "$d"
expect_equal "a failed dry run of the bindings stops before the wrapper" \
    "publish --dry-run $BT --tag beta --access public" "$(publish_log "$d")"

fresh "$VERSION"
tarballs "$WORK/other" 0.1.0-beta.2
cp "$WORK/other/fedimint-react-native-0.1.0-beta.2.tgz" "$d/"
rm "${d:?}/$NT"
expect_fail "tarballs of different versions are refused" "0.1.0-beta.1" -- "$publish" "$d"
expect_fail "the other version is named" "0.1.0-beta.2" -- "$publish" "$d"
expect_equal "nothing is published for tarballs of different versions" "" "$(publish_log "$d")"

fresh "$VERSION"
cp "$WORK/other/fedimint-react-native-bindings-0.1.0-beta.2.tgz" "$d/"
expect_fail "two bindings tarballs are refused" "found 2" -- "$publish" "$d"
fresh "$VERSION"
cp "$WORK/other/fedimint-react-native-0.1.0-beta.2.tgz" "$d/"
expect_fail "two wrapper tarballs are refused" "found 2" -- "$publish" "$d"
d="$(mktemp -d "$WORK/pub.XXXX")"
expect_fail "no tarball is refused" "found no" -- "$publish" "$d"
echo old >"$d/$NT"
expect_fail "a directory with only a wrapper tarball is refused" "found no" -- "$publish" "$d"
expect_fail "a directory that does not exist is refused" "$WORK/nothing" -- \
    "$publish" "$WORK/nothing"

fresh "$VERSION"
rewrite "$d/$BT" edit_manifest 'm.name = "@fedimint/other"'
expect_fail "a bindings tarball holding another package is refused" "@fedimint/other" -- \
    "$publish" "$d"
expect_equal "nothing is published for a tarball holding another package" "" "$(publish_log "$d")"
fresh "$VERSION"
rewrite "$d/$NT" edit_manifest 'm.name = "@fedimint/react-native-bindings"'
expect_fail "a wrapper tarball holding another package is refused" "not $WRAPPER" -- \
    "$publish" "$d"

# A publish that fails while another upload of the same version landed: what is on npm is not
# this tarball, so the release stops there, before the wrapper is attempted, and the local file
# stays as it is.
fresh "$VERSION"
: >"$FAKE_NPM/publish-fails"
mkdir "$FAKE_NPM/published"
marker="$FAKE_NPM/published/${BINDINGS//\//__}@$VERSION"
echo "published by someone else" >"$marker.tgz"
integrity "$marker.tgz" >"$marker.integrity"
# The fake only says a version is on npm once its marker exists, so the bindings appear between
# the first question and the failing publish. Only they do, so that nothing after the failed
# publish can stand in for the check made there.
cat >"$WORK/bin/npm-appears" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == publish ]]; then
    for marker in "$FAKE_NPM"/published/*.tgz; do
        : >"${marker%.tgz}"
    done
fi
exec "$REAL_FAKE_NPM" "$@"
EOF
chmod +x "$WORK/bin/npm-appears"
mkdir "$WORK/appears"
ln -s "$WORK/bin/npm-appears" "$WORK/appears/npm"
cp "$d/$BT" "$WORK/bindings-before.tgz"
REAL_FAKE_NPM="$WORK/bin/npm" PATH="$WORK/appears:$PATH" run "$publish" "$d"
expect_equal "a publish that fails over a version someone else published fails the release" 1 \
    "$STATUS"
expect_contains "the refusal names the bindings" \
    "$BINDINGS@$VERSION is on npm as another tarball" "$OUT"
expect_equal "the version on npm is not taken for this tarball" no \
    "$([[ "$OUT" == *"on npm although"* ]] && echo yes || echo no)"
expect_equal "the local bindings tarball is unchanged after such a publish" same \
    "$(same "$d/$BT" "$WORK/bindings-before.tgz")"
expect_equal "only the bindings were attempted" "publish $BT --tag beta --access public" \
    "$(publish_log "$d")"
expect_equal "no SHA256SUMS is written after such a publish" absent "$(sums_state "$d")"

# A wrapper that someone else publishes while the bindings are uploaded: both packages passed
# before the upload, and the wrapper is looked at again when its turn comes.
fresh "$VERSION"
mkdir "$FAKE_NPM/published"
marker="$FAKE_NPM/published/${WRAPPER//\//__}@$VERSION"
echo "published by someone else" >"$marker.tgz"
integrity "$marker.tgz" >"$marker.integrity"
REAL_FAKE_NPM="$WORK/bin/npm" PATH="$WORK/appears:$PATH" run "$publish" "$d"
expect_equal "a wrapper that appears on npm during the release fails it" 1 "$STATUS"
expect_contains "the refusal names the wrapper that appeared" \
    "$WRAPPER@$VERSION is on npm as another tarball" "$OUT"
expect_equal "the wrapper that appeared is not skipped as if it were this one" no \
    "$([[ "$OUT" == *"$WRAPPER@$VERSION is already on npm, skipped"* ]] && echo yes || echo no)"
expect_equal "only the bindings were uploaded before the wrapper appeared" \
    "publish $BT --tag beta --access public" "$(publish_log "$d")"
expect_equal "no SHA256SUMS is written when the wrapper appeared" absent "$(sums_state "$d")"

# Both packages are looked at before anything is uploaded.
fresh "$VERSION"
echo "what npm serves" >"$served"
on_registry "$WRAPPER" "$VERSION" "$served"
run "$publish" "$d"
expect_equal "a wrapper on npm as another tarball: the release fails" 1 "$STATUS"
expect_contains "the refusal names the wrapper" "$WRAPPER@$VERSION is on npm as another tarball" \
    "$OUT"
expect_equal "the bindings are not uploaded first" "" "$(publish_log "$d")"

fresh "$VERSION"
on_registry "$BINDINGS" "$VERSION" "$d/$BT"
on_registry "$WRAPPER" "$VERSION" "$served"
run "$publish" "$d"
expect_equal "same bindings and another wrapper on npm: the release fails" 1 "$STATUS"
expect_contains "the refusal names the wrapper when the bindings match" \
    "$WRAPPER@$VERSION is on npm as another tarball" "$OUT"
expect_equal "nothing is published next to a wrapper that is another tarball" "" \
    "$(publish_log "$d")"
expect_equal "no SHA256SUMS is written for a wrapper that is another tarball" absent \
    "$(sums_state "$d")"

# A dist-tag never moves back.
fresh "$VERSION"
registry_tags "$BINDINGS" '{"latest":"0.0.0","beta":"0.1.0-beta.2"}'
run "$publish" "$d"
expect_equal "bindings whose beta is on a newer version: the release fails" 1 "$STATUS"
expect_contains "the refusal names the package, the dist-tag and the version it is on" \
    "the dist-tag beta of $BINDINGS is on 0.1.0-beta.2" "$OUT"
expect_contains "the refusal names the version being published" "0.1.0-beta.1 is not newer" "$OUT"
expect_equal "nothing is published when the bindings' beta is ahead" "" "$(publish_log "$d")"

fresh "$VERSION"
registry_tags "$BINDINGS" '{"latest":"0.0.0","beta":"0.0.9-beta.3"}'
registry_tags "$WRAPPER" '{"latest":"0.0.0","beta":"0.1.0-beta.2"}'
run "$publish" "$d"
expect_equal "a wrapper whose beta is on a newer version: the release fails" 1 "$STATUS"
expect_contains "the refusal names the wrapper's beta" \
    "the dist-tag beta of $WRAPPER is on 0.1.0-beta.2" "$OUT"
expect_equal "the bindings are not uploaded before the wrapper's dist-tag is read" "" \
    "$(publish_log "$d")"

fresh "$VERSION"
on_registry "$BINDINGS" "$VERSION" "$d/$BT"
registry_tags "$WRAPPER" '{"latest":"0.0.0","beta":"0.1.0-beta.2"}'
run "$publish" "$d"
expect_equal "an older release run again after a newer one: the release fails" 1 "$STATUS"
expect_contains "the older release is refused over the wrapper's dist-tag" \
    "the dist-tag beta of $WRAPPER is on 0.1.0-beta.2" "$OUT"
expect_equal "nothing is published for an older release run again" "" "$(publish_log "$d")"

fresh "$VERSION"
registry_tags "$BINDINGS" '{"latest":"0.0.0","beta":"0.0.9-beta.3","canary":"0.0.0-canary-abc123"}'
registry_tags "$WRAPPER" '{"latest":"0.0.0","beta":"0.0.9-beta.3","canary":"0.0.0-canary-abc123"}'
run "$publish" "$d"
expect_equal "dist-tags on older versions: the release succeeds" 0 "$STATUS"
expect_equal "both packages are published under beta over an older beta" \
    "publish $BT --tag beta --access public"$'\n'"publish $NT --tag beta --access public" \
    "$(publish_log "$d")"

fresh "$VERSION"
registry_tags "$BINDINGS" '{"latest":"0.0.0"}'
registry_tags "$WRAPPER" '{"latest":"0.0.0"}'
run "$publish" "$d"
expect_equal "packages without a beta dist-tag: the release succeeds" 0 "$STATUS"
expect_equal "both packages are published under beta when there is none yet" \
    "publish $BT --tag beta --access public"$'\n'"publish $NT --tag beta --access public" \
    "$(publish_log "$d")"

fresh 0.1.0
registry_tags "$BINDINGS" '{"latest":"0.0.0","beta":"0.2.0-beta.1"}'
registry_tags "$WRAPPER" '{"latest":"0.0.0","beta":"0.2.0-beta.1"}'
run "$publish" "$d"
expect_equal "a final version with a beta that is ahead: the release succeeds" 0 "$STATUS"
expect_equal "both packages are published under latest, whatever beta is on" \
    "publish $BT0 --tag latest --access public"$'\n'"publish $NT0 --tag latest --access public" \
    "$(publish_log "$d")"

fresh 0.1.0
registry_tags "$BINDINGS" '{"latest":"0.2.0"}'
run "$publish" "$d"
expect_equal "a final version below latest: the release fails" 1 "$STATUS"
expect_contains "the refusal names latest" "the dist-tag latest of $BINDINGS is on 0.2.0" "$OUT"
expect_equal "nothing is published below latest" "" "$(publish_log "$d")"

# A dist-tag can be on a version that is no release version. It counts as the release it starts
# with, as any version on a registry does.
fresh 0.1.0
registry_tags "$BINDINGS" '{"latest":"0.2.0-rc.1"}'
run "$publish" "$d"
expect_equal "a final version below a latest outside the grammar: the release fails" 1 "$STATUS"
expect_contains "the refusal names the version latest is on, as npm has it" \
    "the dist-tag latest of $BINDINGS is on 0.2.0-rc.1" "$OUT"
expect_equal "nothing is published below a latest outside the grammar" "" "$(publish_log "$d")"

# A dist-tag that cannot be read must not look like one that is not there.
fresh "$VERSION"
echo '{"error":{"code":"E500"}}' >"$FAKE_NPM/dist-tags-error.json"
run "$publish" "$d"
expect_equal "dist-tags that cannot be read: the release fails" 1 "$STATUS"
expect_contains "the refusal names the error code" "E500" "$OUT"
expect_equal "nothing is published when the dist-tags cannot be read" "" "$(publish_log "$d")"

for answer in '"0.1.0"' '[]'; do
    fresh "$VERSION"
    registry_tags "$BINDINGS" "$answer"
    run "$publish" "$d"
    expect_equal "dist-tags that are $answer: the release fails" 1 "$STATUS"
    expect_contains "dist-tags that are $answer are refused as unreadable" \
        "which this script does not read" "$OUT"
    expect_equal "nothing is published for dist-tags that are $answer" "" "$(publish_log "$d")"
done

# One registry across two releases: the older one cannot follow the newer one.
fresh "$VERSION"
d1="$d"
d2="$(mktemp -d "$WORK/pub.XXXX")"
tarballs "$d2" 0.1.0-beta.2
BT2=fedimint-react-native-bindings-0.1.0-beta.2.tgz
NT2=fedimint-react-native-0.1.0-beta.2.tgz
run "$publish" "$d2"
expect_equal "the newer beta is published" 0 "$STATUS"
run "$publish" "$d1"
expect_equal "the older beta is refused after the newer one" 1 "$STATUS"
expect_contains "the older beta is refused over the bindings' dist-tag" \
    "the dist-tag beta of $BINDINGS is on 0.1.0-beta.2" "$OUT"
expect_equal "only the two uploads of the newer beta were made" \
    "publish $BT2 --tag beta --access public"$'\n'"publish $NT2 --tag beta --access public" \
    "$(publish_log "$d2")"
expect_equal "no SHA256SUMS is written for the refused older beta" absent "$(sums_state "$d1")"
tag_reads="$(grep -c dist-tags "$FAKE_NPM/calls" || true)"
run "$publish" "$d2"
expect_equal "the newer beta run again: the release succeeds" 0 "$STATUS"
expect_contains "the newer beta skips the bindings" \
    "$BINDINGS@0.1.0-beta.2 is already on npm, skipped" "$OUT"
expect_contains "the newer beta skips the wrapper" \
    "$WRAPPER@0.1.0-beta.2 is already on npm, skipped" "$OUT"
expect_equal "the dist-tags of versions that are on npm are not read" "$tag_reads" \
    "$(grep -c dist-tags "$FAKE_NPM/calls" || true)"
expect_equal "running the newer beta again uploads nothing" 2 \
    "$(wc -l <"$FAKE_NPM/log" | tr -d ' ')"

fresh 1.0.0-rc.1
expect_fail "tarballs of a version outside the grammar are refused" \
    "must be X.Y.Z-beta.N or X.Y.Z" -- "$publish" "$d"
expect_equal "nothing is published for a version outside the grammar" "" "$(publish_log "$d")"
fresh 1.0.0-rc.1
: >"$FAKE_NPM/forbid-view"
expect_fail "a dry run refuses a version outside the grammar too" \
    "must be X.Y.Z-beta.N or X.Y.Z" -- "$publish" --dry-run "$d"

fresh "$VERSION"
: >"$FAKE_NPM/view-empty"
run "$publish" "$d"
expect_equal "an empty answer from npm view counts as not published" \
    "publish $BT --tag beta --access public"$'\n'"publish $NT --tag beta --access public" \
    "$(publish_log "$d")"

echo "usage"

expect_usage "no arguments" -- "$publish"
expect_usage "--dry-run without a directory" -- "$publish" --dry-run
expect_usage "two directories" -- "$publish" "$d" "$d"
expect_usage "an option other than --dry-run" -- "$publish" --tag "$d"
expect_usage "--dry-run after the directory" -- "$publish" "$d" --dry-run
expect_usage "--dry-run with two directories" -- "$publish" --dry-run "$d" "$d"

echo
echo "react-native-sdk-example.sh"

example="$ROOT/scripts/react-native-sdk-example.sh"
EXAMPLE_SRC="$ROOT/js/examples/react-native"

# blocked_above <dir>: prints the nearest ancestor of <dir> that holds a package.json, a
# pnpm-workspace.yaml or a node_modules directory, or nothing when there is none.
blocked_above() {
    local dir
    dir="$(cd "$1" && pwd -P)"
    while [[ "$dir" != / ]]; do
        dir="$(dirname "$dir")"
        if [[ -e "$dir/package.json" || -e "$dir/pnpm-workspace.yaml" ||
            -e "$dir/node_modules" ]]; then
            echo "$dir"
            return
        fi
    done
}

# manifest_value <package.json> <javascript>: the value of the expression over the manifest, `p`.
manifest_value() {
    node -e '
        const p = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
        console.log(new Function("p", "return " + process.argv[2])(p));
    ' "$1" "$2"
}

# present <path>: prints `present` or `absent`.
present() {
    if [[ -e "$1" ]]; then
        echo present
    else
        echo absent
    fi
}

d="$(valid_pair)"
stage_parent="$(mktemp -d "$WORK/app.XXXX")"
app="$stage_parent/app"
blocker="$(blocked_above "$stage_parent")"
if [[ -n "$blocker" ]]; then
    echo "  skip  staging an app: $blocker holds a package.json, a pnpm-workspace.yaml or a" \
        "node_modules directory above the test's work directory"
else
    run "$example" --no-install "$app" "$d/$BT" "$d/$NT"
    expect_equal "staging into a fresh directory succeeds" 0 "$STATUS"
    expect_equal "it prints where it staged the app last" "staged $app" "${OUT##*$'\n'}"
    expect_equal "the bindings are a file: dependency on the tarball" "file:$d/$BT" \
        "$(manifest_value "$app/package.json" 'p.dependencies["@fedimint/react-native-bindings"]')"
    expect_equal "the wrapper is a file: dependency on the tarball" "file:$d/$NT" \
        "$(manifest_value "$app/package.json" 'p.dependencies["@fedimint/react-native"]')"
    expect_equal "every other dependency is an exact version" "" "$(manifest_value \
        "$app/package.json" 'Object.entries({ ...p.dependencies, ...p.devDependencies })
            .filter(([n, v]) => !v.startsWith("file:") &&
                !/^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$/.test(v))
            .map(([n, v]) => n + "@" + v).join(" ")')"
    expect_equal "react-native is the version the example names" \
        "$(manifest_value "$EXAMPLE_SRC/package.json" 'p.dependencies["react-native"]')" \
        "$(manifest_value "$app/package.json" 'p.dependencies["react-native"]')"
    expect_equal "no dependency of the example is lost or added" \
        "$(manifest_value "$EXAMPLE_SRC/package.json" \
            'Object.keys({ ...p.dependencies, ...p.devDependencies }).sort().join(" ")')" \
        "$(manifest_value "$app/package.json" \
            'Object.keys({ ...p.dependencies, ...p.devDependencies }).sort().join(" ")')"
    expect_equal "the rest of the manifest is kept" \
        "$(manifest_value "$EXAMPLE_SRC/package.json" \
            'JSON.stringify([p.name, p.private, p.scripts, p.engines])')" \
        "$(manifest_value "$app/package.json" \
            'JSON.stringify([p.name, p.private, p.scripts, p.engines])')"
    expect_equal "the manifest has two-space indentation and a final newline" same \
        "$(node -e '
            const fs = require("fs");
            const text = fs.readFileSync(process.argv[1], "utf8");
            const again = JSON.stringify(JSON.parse(text), null, 2) + "\n";
            console.log(text === again ? "same" : "other");
        ' "$app/package.json")"
    expect_equal "the self-check is the entry file index.smoke.js" same \
        "$(same "$app/index.smoke.js" "$ROOT/scripts/react-native-sdk-smoke.js")"
    expect_equal "the example's own entry file is kept" same \
        "$(same "$app/index.js" "$EXAMPLE_SRC/index.js")"
    expect_equal "react-native.config.js is removed" absent \
        "$(present "$app/react-native.config.js")"
    expect_equal "ios/Podfile.lock is removed" absent "$(present "$app/ios/Podfile.lock")"
    expect_equal "metro.config.js names no workspace folder" 0 \
        "$(grep -c 'watchFolders\|extraNodeModules' "$app/metro.config.js" || true)"
    expect_contains "metro.config.js is the stock one" \
        "module.exports = mergeConfig(getDefaultConfig(__dirname), config);" \
        "$(cat "$app/metro.config.js")"
    expect_equal "android/gradlew is executable" yes \
        "$([[ -x "$app/android/gradlew" ]] && echo yes || echo no)"
    expect_equal "the debug keystore is there" present \
        "$(present "$app/android/app/debug.keystore")"
    expect_equal "the example's source is there" present "$(present "$app/src/App.tsx")"
    expect_equal "no node_modules directory is staged" "" \
        "$(find "$app" -name node_modules -print)"

    empty="$stage_parent/empty"
    mkdir "$empty"
    expect_last "an empty existing directory is accepted" "staged $empty" -- \
        "$example" --no-install "$empty" "$d/$BT" "$d/$NT"
fi

echo "refusals"

# The cases below stop before anything is written, so they do not depend on what is above the
# work directory.
d="$(valid_pair)"
non_empty="$(mktemp -d "$WORK/full.XXXX")"
echo keep >"$non_empty/file"
expect_fail "a destination that is not empty is refused" "is not empty" -- \
    "$example" --no-install "$non_empty" "$d/$BT" "$d/$NT"
expect_equal "it leaves that directory as it was" "file" "$(ls "$non_empty")"
inside="$ROOT/js/rn-example-test-destination"
expect_fail "a destination inside this repository is refused" "inside this repository" -- \
    "$example" --no-install "$inside" "$d/$BT" "$d/$NT"
expect_equal "it creates nothing there" absent "$(present "$inside")"
expect_fail "a destination in a subdirectory of this repository is refused" \
    "inside this repository" -- \
    "$example" --no-install "$ROOT/js/examples/react-native" "$d/$BT" "$d/$NT"
expect_fail "a destination whose parent does not exist is refused" "parent directory" -- \
    "$example" --no-install "$WORK/no-such-parent/app" "$d/$BT" "$d/$NT"

for marker in package.json pnpm-workspace.yaml node_modules; do
    above="$(mktemp -d "$WORK/above.XXXX")"
    mkdir -p "$above/deeper"
    if [[ "$marker" == node_modules ]]; then
        mkdir "$above/$marker"
    else
        echo '{}' >"$above/$marker"
    fi
    expect_fail "an ancestor holding $marker is refused" "$above holds a $marker" -- \
        "$example" --no-install "$above/deeper/app" "$d/$BT" "$d/$NT"
    expect_equal "it creates nothing below an ancestor holding $marker" absent \
        "$(present "$above/deeper/app")"
done

clean="$(mktemp -d "$WORK/clean.XXXX")"
expect_fail "a tarball that does not exist is refused" "$d/missing.tgz does not exist" -- \
    "$example" --no-install "$clean/app" "$d/missing.tgz" "$d/$NT"
expect_fail "a wrapper tarball that does not exist is refused" "$d/missing.tgz does not exist" -- \
    "$example" --no-install "$clean/app" "$d/$BT" "$d/missing.tgz"
expect_fail "the tarballs in the wrong order are refused" \
    "holds $WRAPPER, not $BINDINGS" -- \
    "$example" --no-install "$clean/app" "$d/$NT" "$d/$BT"
expect_fail "a file that is not a tarball is refused" "no usable package/package.json" -- \
    "$example" --no-install "$clean/app" "$WORK/bin/npm" "$d/$NT"
tarballs "$WORK/other-version" 0.1.0-beta.2
expect_fail "tarballs of different versions are refused" "for different versions" -- \
    "$example" --no-install "$clean/app" "$d/$BT" \
    "$WORK/other-version/fedimint-react-native-0.1.0-beta.2.tgz"
expect_equal "none of these refusals created the destination" absent "$(present "$clean/app")"

echo "the lockfile"

# A throwaway tree laid out like this repository, holding a copy of the script, the example's
# tracked files and the workspace lockfile, so that a case can damage them. Prints its path.
example_tree() {
    local dir
    dir="$(mktemp -d "$WORK/extree.XXXX")"
    mkdir -p "$dir/scripts" "$dir/js/examples/react-native"
    cp "$ROOT/scripts/react-native-sdk-example.sh" "$ROOT/scripts/react-native-sdk-smoke.js" \
        "$dir/scripts/"
    cp "$ROOT/js/pnpm-lock.yaml" "$dir/js/"
    (cd "$EXAMPLE_SRC" && git ls-files -z | tar --null -T - -cf -) |
        (cd "$dir/js/examples/react-native" && tar -xf -)
    git -C "$dir" init -q -b main
    git -C "$dir" config user.name test
    git -C "$dir" config user.email test@example.invalid
    git -C "$dir" config commit.gpgSign false
    git -C "$dir" add -A
    git -C "$dir" commit -q -m base
    echo "$dir"
}

# edit_lockfile <tree> <javascript> [<argument>]: replaces the lockfile's text, `t`, with the code's
# result. The code finds the argument in process.argv[3].
edit_lockfile() {
    node -e '
        const fs = require("fs");
        const t = fs.readFileSync(process.argv[2], "utf8");
        fs.writeFileSync(process.argv[2], new Function("t", process.argv[1])(t));
    ' "$2" "$1/js/pnpm-lock.yaml" "${3:-}"
}

# staged_in <tree> <name>: stages the tree's example into a fresh directory $app below $clean.
staged_in() {
    app="$clean/$2"
    run "$1/scripts/react-native-sdk-example.sh" --no-install "$app" "$d/$BT" "$d/$NT"
}

tree="$(example_tree)"
staged_in "$tree" unmodified
expect_equal "a copy of the real tree stages the same way" 0 "$STATUS"

tree="$(example_tree)"
edit_lockfile "$tree" 'return t.replace("\n  examples/react-native:\n", "\n  examples/other:\n")'
staged_in "$tree" no-importer
expect_equal "a lockfile without the example's importer fails" 1 "$STATUS"
expect_contains "it says which importer is missing" "examples/react-native" "$OUT"
expect_contains "it says it left the staged copy in place" "left in $app" "$OUT"
expect_equal "the staged copy is there as evidence" present "$(present "$app/package.json")"

tree="$(example_tree)"
edit_manifest 'm.dependencies["left-pad"] = "^1.0.0"' "$tree/js/examples/react-native"
staged_in "$tree" no-version
expect_equal "a dependency the lockfile does not record fails" 1 "$STATUS"
expect_contains "it names the dependency" "no version for left-pad " "$OUT"

tree="$(example_tree)"
edit_manifest 'm.dependencies["@fedimint/sibling"] = "workspace:*"' "$tree/js/examples/react-native"
edit_lockfile "$tree" 'return t.replace("  examples/react-native:\n    dependencies:\n",
    "  examples/react-native:\n    dependencies:\n      '@fedimint/sibling':\n" +
    "        specifier: workspace:*\n        version: link:../../sibling\n")'
staged_in "$tree" linked
expect_equal "a dependency linked to the workspace fails" 1 "$STATUS"
expect_contains "it names the linked dependency" "@fedimint/sibling" "$OUT"
expect_contains "it says what the link is" "link:../../sibling" "$OUT"

# lock_react <tree> <version line>: replaces the `version:` line of react in the example's
# importer with the text, which is nothing when the line is to go. The JavaScript is not meant to
# be expanded by the shell.
# shellcheck disable=SC2016
lock_react() {
    edit_lockfile "$1" 'const re = new RegExp(
        "(\\n  examples/react-native:\\n[\\s\\S]*?" +
        "\\n      react:\\n {8}specifier: [^\\n]*\\n)( {8}version: [^\\n]*\\n)?");
        return t.replace(re, "$1" + process.argv[3])' "$2"
}

tree="$(example_tree)"
lock_react "$tree" '        version: 18.0.0(foo@1)(bar@2)'$'\n'
staged_in "$tree" peers
expect_equal "a version with several peer sets stages" 0 "$STATUS"
expect_equal "it keeps what precedes the first parenthesis" 18.0.0 \
    "$(manifest_value "$app/package.json" 'p.dependencies.react')"

tree="$(example_tree)"
lock_react "$tree" ""
staged_in "$tree" no-version-line
expect_equal "a dependency without a version line fails" 1 "$STATUS"
expect_contains "it names that dependency too" "no version for react " "$OUT"

echo "usage"

expect_usage "no arguments" -- "$example"
expect_usage "only --no-install" -- "$example" --no-install
expect_usage "one argument" -- "$example" "$clean/app"
expect_usage "two arguments" -- "$example" "$clean/app" "$d/$BT"
expect_usage "four arguments" -- "$example" "$clean/app" "$d/$BT" "$d/$NT" extra
expect_usage "an unknown option" -- "$example" --install "$clean/app" "$d/$BT" "$d/$NT"
expect_usage "--no-install after the destination" -- \
    "$example" "$clean/app" --no-install "$d/$BT" "$d/$NT"

echo
echo "react-native-sdk-smoke.sh"

smoke="$ROOT/scripts/react-native-sdk-smoke.sh"
APK="$WORK/app-release.apk"
echo apk >"$APK"
APP=com.reactnativeexample

# The fake adb records every call in $FAKE_ADB/log and answers from files in $FAKE_ADB:
#   get-state            `device`, or a failure when no-device exists
#   shell pm path android, install -r <apk>, shell pm clear <app>, logcat -c, shell am start ...
#                        succeed; install fails when install-fails exists
#   logcat -v time ...   writes its pid to logcat.pid, waits for logcat.txt to exist, prints it and
#                        whatever is appended to it, and runs until it is killed
#   logcat -d -t 200     `device log tail`
#   shell pidof <app>    `4242`, or nothing when dead exists
# Any other call fails loudly.
mkdir "$WORK/adb-bin"
cat >"$WORK/adb-bin/adb" <<'EOF'
#!/usr/bin/env bash
echo "$*" >>"$FAKE_ADB/log"
case "$*" in
    get-state)
        if [[ -f "$FAKE_ADB/no-device" ]]; then
            echo "error: no devices/emulators found" >&2
            exit 1
        fi
        echo device
        ;;
    "shell pm path android") echo "package:/system/framework/framework-res.apk" ;;
    "install -r "*)
        if [[ -f "$FAKE_ADB/install-fails" ]]; then
            echo "adb: failed to install" >&2
            exit 1
        fi
        echo Success
        ;;
    "shell pm clear com.reactnativeexample") echo Success ;;
    "logcat -c") ;;
    "logcat -v time ReactNativeJS:V AndroidRuntime:E DEBUG:F libc:F *:S")
        echo $$ >"$FAKE_ADB/logcat.pid"
        while [[ ! -f "$FAKE_ADB/logcat.txt" ]]; do
            sleep 0.2
        done
        exec tail -f -n +1 "$FAKE_ADB/logcat.txt"
        ;;
    "logcat -d -t 200") echo "device log tail" ;;
    "shell am start -W -n com.reactnativeexample/.MainActivity") echo "Status: ok" ;;
    "shell pidof com.reactnativeexample")
        if [[ ! -f "$FAKE_ADB/dead" ]]; then
            echo 4242
        fi
        ;;
    *)
        echo "unexpected adb call: $*" >&2
        exit 9
        ;;
esac
EOF
chmod +x "$WORK/adb-bin/adb"

# Lines as `adb logcat -v time` prints them.
LOG_JS='10-08 12:00:00.123 I/ReactNativeJS(12345): FEDIMINT_RN_SMOKE'
OK_LINE="$LOG_JS "'{"status":"ok","generated":12,"stored":12}'
FAIL_LINE="$LOG_JS "'{"status":"fail","error":"Error: native library missing"}'
CRASH_LINE='10-08 12:00:01.456 E/AndroidRuntime(12345): FATAL EXCEPTION: mqt_v_js'

# The cases leave their fake adb directories here, to check afterwards that nothing they started
# is still running.
smoke_dirs=()

# smoke_case: a fresh fake adb directory, in $fa.
smoke_case() {
    fa="$(mktemp -d "$WORK/adb.XXXX")"
    smoke_dirs+=("$fa")
}

# smoke_start <fake adb dir> <script> <args...>: runs the script in the background against the fake
# adb of the directory, with its output and exit status in files there. The timeouts are short.
smoke_start() {
    local fa="$1" script="$2"
    shift 2
    (
        status=0
        env PATH="$WORK/adb-bin:$PATH" FAKE_ADB="$fa" SMOKE_TIMEOUT="${SMOKE_TIMEOUT:-20}" \
            "$script" "$@" >"$fa/out" 2>&1 || status=$?
        echo "$status" >"$fa/status"
    ) &
    echo $! >"$fa/smoke.pid"
}

# smoke_wait <fake adb dir>: waits for the script started on the directory, and leaves its output
# in $OUT and its exit status in $STATUS.
smoke_wait() {
    wait "$(cat "$1/smoke.pid")"
    OUT="$(cat "$1/out")"
    STATUS="$(cat "$1/status")"
}

# adb_calls <fake adb dir>: the calls the fake adb saw, one per line.
adb_calls() {
    cat "$1/log" 2>/dev/null || true
}

# The cases that wait five seconds after an ok verdict are started together.
smoke_case
ok_dir="$fa"
echo "$OK_LINE" >"$fa/logcat.txt"
smoke_start "$fa" "$smoke" "$APK"

smoke_case
late_dir="$fa"
(sleep 2 && echo "$OK_LINE" >"$late_dir/logcat.txt") &
smoke_start "$fa" "$smoke" "$APK"

smoke_case
then_crash_dir="$fa"
echo "$OK_LINE" >"$fa/logcat.txt"
(sleep 2 && echo "$CRASH_LINE" >>"$then_crash_dir/logcat.txt") &
smoke_start "$fa" "$smoke" "$APK"

smoke_case
then_dead_dir="$fa"
echo "$OK_LINE" >"$fa/logcat.txt"
touch "$fa/dead"
smoke_start "$fa" "$smoke" "$APK"

smoke_case
gone_dir="$fa"
touch "$fa/dead"
SMOKE_TIMEOUT=30 smoke_start "$fa" "$smoke" "$APK"

# --boot runs a copy of the script beside a fake emulator script: the fake records how it was
# started, prints the line that says Android is up and stays until it is killed.
boot_tree="$(mktemp -d "$WORK/boot.XXXX")"
mkdir "$boot_tree/scripts"
cp "$smoke" "$boot_tree/scripts/"
cat >"$boot_tree/scripts/rn-android-emulator.sh" <<'EOF'
#!/usr/bin/env bash
echo "$*" >"$FAKE_ADB/emulator.args"
echo "${RN_ANDROID_EMULATOR_BOOT_TIMEOUT-unset}" >"$FAKE_ADB/emulator.timeout"
echo $$ >"$FAKE_ADB/emulator.pid"
if [[ -f "$FAKE_ADB/emulator-fails" ]]; then
    echo "the fake emulator cannot start" >&2
    exit 1
fi
echo "==> Ready: fake"
exec sleep 600
EOF
chmod +x "$boot_tree/scripts/rn-android-emulator.sh"
boot_smoke="$boot_tree/scripts/react-native-sdk-smoke.sh"

smoke_case
boot_dir="$fa"
echo "$OK_LINE" >"$fa/logcat.txt"
touch "$fa/no-device"
BOOT_TIMEOUT=77 smoke_start "$fa" "$boot_smoke" --boot "$APK"

echo "verdicts"

smoke_wait "$ok_dir"
expect_equal "an ok verdict passes" 0 "$STATUS"
expect_contains "it prints the verdict" '"status":"ok","generated":12,"stored":12' "$OUT"
expect_contains "it says the app runs" "the example app runs with the installed packages" "$OUT"
expect_equal "it installs the apk, clears its data, clears the log, then starts the app" \
    "get-state
shell pm path android
install -r $APK
shell pm clear $APP
logcat -c
shell am start -W -n $APP/.MainActivity" \
    "$(adb_calls "$ok_dir" | grep -v 'pidof\|logcat -v time')"
expect_contains "it follows the log of the app, its crashes and the native ones" \
    "logcat -v time ReactNativeJS:V AndroidRuntime:E DEBUG:F libc:F *:S" "$(adb_calls "$ok_dir")"

smoke_wait "$late_dir"
expect_equal "a verdict that arrives two seconds after the launch passes" 0 "$STATUS"
expect_contains "it is the verdict of that line" "the example app runs" "$OUT"

smoke_case
echo "$FAIL_LINE" >"$fa/logcat.txt"
smoke_start "$fa" "$smoke" "$APK"
smoke_wait "$fa"
expect_equal "a fail verdict fails" 1 "$STATUS"
expect_contains "it prints the error of the verdict" "Error: native library missing" "$OUT"
expect_contains "it says the self-check failed" "the self-check reported a failure" "$OUT"
expect_contains "it prints the end of the device log" "device log tail" "$OUT"

smoke_case
echo "$CRASH_LINE" >"$fa/logcat.txt"
smoke_start "$fa" "$smoke" "$APK"
smoke_wait "$fa"
expect_equal "a crash without a verdict fails" 1 "$STATUS"
expect_contains "it says the app crashed" "the app crashed" "$OUT"
expect_contains "it prints the crash from the captured log" "FATAL EXCEPTION: mqt_v_js" "$OUT"
expect_contains "it prints the end of the device log after a crash" "device log tail" "$OUT"

smoke_case
echo "10-08 12:00:00.123 I/ReactNativeJS(12345): something else" >"$fa/logcat.txt"
SMOKE_TIMEOUT=3 smoke_start "$fa" "$smoke" "$APK"
smoke_wait "$fa"
expect_equal "no verdict within the timeout fails" 1 "$STATUS"
expect_contains "it says there was no verdict and names the timeout" \
    "no verdict within SMOKE_TIMEOUT=3 seconds" "$OUT"
expect_contains "it prints what the app logged" "something else" "$OUT"
expect_contains "it prints the end of the device log on a timeout" "device log tail" "$OUT"

smoke_wait "$then_crash_dir"
expect_equal "an ok verdict followed by a crash fails" 1 "$STATUS"
expect_contains "it says the app crashed after reporting" "crashed after reporting ok" "$OUT"

smoke_wait "$then_dead_dir"
expect_equal "an ok verdict from an app that is gone fails" 1 "$STATUS"
expect_contains "it says the app stopped after reporting" "stopped after reporting ok" "$OUT"
expect_equal "it does not claim success" 0 "$(grep -c 'the example app runs' <<<"$OUT" || true)"

smoke_wait "$gone_dir"
expect_equal "an app that is gone without a verdict fails" 1 "$STATUS"
expect_contains "it says the app is gone" "the app is not running" "$OUT"
expect_equal "it first looks for the process at the tenth second" 1 \
    "$(grep -c pidof "$gone_dir/log")"

echo "device and apk"

smoke_case
touch "$fa/no-device"
smoke_start "$fa" "$smoke" "$APK"
smoke_wait "$fa"
expect_equal "no device and no --boot fails" 1 "$STATUS"
expect_contains "it says how to get a device" "just rn-android-emulator" "$OUT"
expect_contains "it names --boot" "--boot" "$OUT"
expect_equal "it installs nothing" "get-state" "$(adb_calls "$fa")"

smoke_case
smoke_start "$fa" "$smoke" "$WORK/no-such.apk"
smoke_wait "$fa"
expect_equal "an apk that does not exist fails" 1 "$STATUS"
expect_contains "it names the apk" "$WORK/no-such.apk" "$OUT"
expect_equal "it asks adb nothing" "" "$(adb_calls "$fa")"

smoke_case
touch "$fa/install-fails"
smoke_start "$fa" "$smoke" "$APK"
smoke_wait "$fa"
expect_equal "an install that fails fails" 1 "$STATUS"
expect_contains "it says so" "adb install" "$OUT"
expect_equal "it never starts the app" 0 "$(adb_calls "$fa" | grep -c 'am start' || true)"

mkdir "$WORK/no-adb"
ln -s "$(command -v dirname)" "$WORK/no-adb/dirname"
run env PATH="$WORK/no-adb" "$(command -v bash)" "$smoke" "$APK"
expect_equal "a PATH without adb fails" 1 "$STATUS"
expect_contains "it names the shell that has adb" ".#rn-android-emulator" "$OUT"

echo "--boot"

smoke_wait "$boot_dir"
expect_equal "--boot with a device that appears passes" 0 "$STATUS"
expect_equal "the emulator script is started headless and without a snapshot" \
    "-no-window -no-snapshot" "$(cat "$boot_dir/emulator.args")"
expect_equal "it is given the boot timeout" 77 "$(cat "$boot_dir/emulator.timeout")"

smoke_case
touch "$fa/emulator-fails"
SECONDS=0
smoke_start "$fa" "$boot_smoke" --boot "$APK"
smoke_wait "$fa"
expect_equal "--boot with an emulator script that exits fails" 1 "$STATUS"
expect_equal "it fails promptly" yes "$([[ "$SECONDS" -lt 15 ]] && echo yes || echo no)"
expect_contains "it prints the emulator log" "the fake emulator cannot start" "$OUT"
expect_equal "it installs nothing" "" "$(adb_calls "$fa")"

echo "left running"

for fa in "${smoke_dirs[@]}"; do
    for pidfile in logcat.pid emulator.pid; do
        [[ -f "$fa/$pidfile" ]] || continue
        if kill -0 "$(cat "$fa/$pidfile")" 2>/dev/null; then
            fail "${fa##*/}: the process in $pidfile is still running"
            kill "$(cat "$fa/$pidfile")" 2>/dev/null || true
        else
            pass "${fa##*/}: the process in $pidfile is gone"
        fi
    done
done

echo "usage"

expect_usage "no arguments" -- "$smoke"
expect_usage "an unknown option" -- "$smoke" --reboot "$APK"
expect_usage "--boot without an apk" -- "$smoke" --boot
expect_usage "two apks" -- "$smoke" "$APK" "$APK"
expect_usage "--boot after the apk" -- "$smoke" "$APK" --boot

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
