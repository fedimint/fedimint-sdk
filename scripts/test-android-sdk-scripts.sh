#!/usr/bin/env bash
#
# Tests the Android SDK's release gates, scripts/android-sdk-version.sh and
# scripts/android-sdk-changelog.sh. Those two decide whether a release can be
# tagged and what notes it carries, so every rule they enforce has a case
# here. .github/workflows/android-sdk-scripts.yaml runs this on pull requests.
#
#   scripts/test-android-sdk-scripts.sh
#
# Each case runs the real scripts against a throwaway git repository holding
# only a copy of them, a version catalog and a changelog. Nothing in this
# checkout is touched, and nothing beyond bash, git and the POSIX tools the
# scripts use is needed.
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

# A fresh repository with the catalog at version $1 and the given changelog
# (stdin), committed on main. Prints its path.
repo() {
    local dir
    dir="$(mktemp -d "$WORK/repo.XXXX")"
    mkdir -p "$dir/scripts" "$dir/android/gradle"
    cp "$ROOT/scripts/android-sdk-version.sh" "$ROOT/scripts/android-sdk-changelog.sh" "$dir/scripts/"
    printf '[versions]\nfedimintSdk = "%s"\n' "$1" >"$dir/android/gradle/libs.versions.toml"
    cat >"$dir/android/CHANGELOG.md"
    git -C "$dir" init -q -b main
    git -C "$dir" config user.name test
    git -C "$dir" config user.email test@example.invalid
    git -C "$dir" add -A
    git -C "$dir" commit -q -m base
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

# expect_ok <name> <expected stdout> -- <command...>
expect_ok() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        if [[ "$out" == "$want" ]]; then pass "$name"; else fail "$name" "expected: $want"$'\n'"got: $out"; fi
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

finished_notes() {
    cat <<'EOF'
# Changelog

Intro.

## 0.1.0-beta.1

First beta.

- Something users can do.
EOF
}

echo "android-sdk-version.sh"

r="$(repo 0.1.0-beta.1 < <(finished_notes))"
v="$r/scripts/android-sdk-version.sh"
expect_ok "current reads the catalog" "0.1.0-beta.1" -- "$v" current
expect_ok "next beta from a beta" "0.1.0-beta.2" -- "$v" next beta
expect_ok "next release from a beta" "0.1.0" -- "$v" next release
expect_fail "patch refused during a beta" "is a beta" -- "$v" next patch
expect_fail "minor --beta refused during a beta" "is a beta" -- "$v" next minor --beta
expect_fail "--beta refused with beta" "only goes with" -- "$v" next beta --beta
expect_fail "unknown bump refused" "usage" -- "$v" next sideways

"$v" set 1.9.9
expect_ok "set writes the catalog" "1.9.9" -- "$v" current
expect_ok "next patch from a release" "1.9.10" -- "$v" next patch
expect_ok "next minor from a release" "1.10.0" -- "$v" next minor
expect_ok "next major --beta from a release" "2.0.0-beta.1" -- "$v" next major --beta
expect_fail "beta refused from a release" "is a release" -- "$v" next beta
expect_fail "release refused from a release" "is a release" -- "$v" next release
for bad in 0.1.0-alpha.1 0.1.0-rc.1 0.1.0-beta.0 0.1.0-beta 01.2.3 0.1 main-SNAPSHOT; do
    expect_fail "set refuses $bad" "must be X.Y.Z-beta.N or X.Y.Z" -- "$v" set "$bad"
done
expect_ok "a refused set leaves the catalog alone" "1.9.9" -- "$v" current

sed -i.bak 's/^fedimintSdk = .*/fedimintSdk = "0.2.0-rc.1"/' "$r/android/gradle/libs.versions.toml"
expect_fail "current refuses an invalid catalog version" "must be X.Y.Z-beta.N or X.Y.Z" -- "$v" current

newer() { printf '%s\n' "$1" | "$v" check-newer "${@:2}"; }
expect_ok "check-newer with nothing released" "0.1.0-beta.1: nothing is released yet" -- newer "" 0.1.0-beta.1
expect_ok "check-newer: next beta" "0.1.0-beta.2 is newer than the latest release, 0.1.0-beta.1" -- newer "0.1.0-beta.1" 0.1.0-beta.2
expect_ok "check-newer: release after its betas" "0.1.0 is newer than the latest release, 0.1.0-beta.3" -- newer $'0.1.0-beta.1\n0.1.0-beta.3' 0.1.0
expect_fail "check-newer: a released version is taken" "already released" -- newer "0.1.0-beta.1" 0.1.0-beta.1
expect_fail "check-newer: a beta after its release" "is lower than 0.1.0" -- newer "0.1.0" 0.1.0-beta.2
expect_fail "check-newer compares numbers, not text" "is lower than 0.10.0" -- newer $'0.2.0\n0.10.0\n0.9.1' 0.9.2
expect_ok "check-newer: beta.10 is above beta.9" "0.1.0-beta.11 is newer than the latest release, 0.1.0-beta.10" -- newer $'0.1.0-beta.9\n0.1.0-beta.10' 0.1.0-beta.11
expect_ok "check-newer --except leaves the tag's own version out" "0.1.0-beta.2 is newer than the latest release, 0.1.0-beta.1" -- newer $'0.1.0-beta.1\n0.1.0-beta.2' 0.1.0-beta.2 --except 0.1.0-beta.2
expect_ok "latest ignores what is not a version" "0.1.0-beta.1" -- bash -c "printf 'foo\n0.1.0-alpha.1\n0.1.0-beta.1\n' | '$v' latest 2>/dev/null"

echo "android-sdk-changelog.sh"

r="$(repo 0.1.0-beta.1 < <(finished_notes))"
c="$r/scripts/android-sdk-changelog.sh"
expect_ok "section prints the notes without blank edges" $'First beta.\n\n- Something users can do.' -- "$c" section 0.1.0-beta.1
expect_ok "check passes finished notes" "android/CHANGELOG.md has notes for 0.1.0-beta.1" -- "$c" check 0.1.0-beta.1
expect_fail "check fails a missing section" "has no \`## 0.1.0-beta.2\` section" -- "$c" check 0.1.0-beta.2
printf '\n## 0.0.9\n\n' >>"$r/android/CHANGELOG.md"
expect_fail "check fails an empty section" "section is empty" -- "$c" check 0.0.9

# Releases 0.1.0-beta.1, then adds commits: SDK sources, both Cargo
# lockfiles and flake.lock count; JS changes and the bump commit do not.
git -C "$r" tag android-sdk-v0.1.0-beta.1
change "$r" rust/fedimint-sdk/src/lib.rs "feat(sdk): a new call"
change "$r" rust/fedimint-sdk/Cargo.lock "chore(deps): bump a crate in the SDK lockfile"
change "$r" rust/uniffi-bindgen/Cargo.lock "chore(deps): bump uniffi-bindgen's lockfile"
change "$r" flake.lock "chore(nix): update the flake inputs"
change "$r" js/web/index.ts "feat(web): unrelated to Android"
change "$r" android/gradle/libs.versions.toml "chore(android): bump the Android SDK to 0.1.0-beta.2 (#9)"
"$c" draft 0.1.0-beta.2 0.1.0-beta.1
notes="$("$c" section 0.1.0-beta.2)"
for subject in "feat(sdk): a new call" \
    "chore(deps): bump a crate in the SDK lockfile" \
    "chore(deps): bump uniffi-bindgen's lockfile" \
    "chore(nix): update the flake inputs"; do
    if [[ "$notes" == *"- $subject ("* ]]; then pass "draft lists: $subject"; else fail "draft lists: $subject" "$notes"; fi
done
for subject in "feat(web): unrelated to Android" "bump the Android SDK to"; do
    if [[ "$notes" != *"$subject"* ]]; then pass "draft leaves out: $subject"; else fail "draft leaves out: $subject" "$notes"; fi
done
expect_fail "check refuses a draft" "still a draft" -- "$c" check 0.1.0-beta.2
first="$(grep -m1 '^## ' "$r/android/CHANGELOG.md")"
if [[ "$first" == "## 0.1.0-beta.2" ]]; then pass "draft goes above the newest section"; else fail "draft goes above the newest section" "first heading: $first"; fi
expect_ok "draft leaves older sections alone" $'First beta.\n\n- Something users can do.' -- "$c" section 0.1.0-beta.1
expect_fail "draft refuses a version that has a section" "already has" -- "$c" draft 0.1.0-beta.2 0.1.0-beta.1
expect_fail "draft needs the previous release's tag" "no tag android-sdk-v0.0.1" -- "$c" draft 0.2.0 0.0.1

r="$(repo 0.1.0-beta.1 < <(printf '# Changelog\n\nIntro.\n'))"
c="$r/scripts/android-sdk-changelog.sh"
"$c" draft 0.1.0-beta.1
if "$c" section 0.1.0-beta.1 | grep -q 'nothing is released yet'; then
    pass "draft with nothing released asks for first-release notes"
else
    fail "draft with nothing released asks for first-release notes" "$(cat "$r/android/CHANGELOG.md")"
fi

r="$(repo 0.1.0-beta.1 < <(finished_notes))"
c="$r/scripts/android-sdk-changelog.sh"
git -C "$r" tag android-sdk-v0.1.0-beta.1
change "$r" js/web/index.ts "feat(web): unrelated to Android"
"$c" draft 0.1.0-beta.2 0.1.0-beta.1
if "$c" section 0.1.0-beta.2 | grep -q 'No changes to the SDK since 0.1.0-beta.1'; then
    pass "draft says so when nothing touched the SDK"
else
    fail "draft says so when nothing touched the SDK" "$("$c" section 0.1.0-beta.2)"
fi

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
