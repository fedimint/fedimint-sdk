#!/usr/bin/env bash
#
# Tests the SDK release gates, scripts/android-sdk-version.sh and
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

# A fresh repository holding only a copy of the two scripts and a base commit on main: no
# version catalog and no android/CHANGELOG.md. Prints its path.
bare_repo() {
    local dir
    dir="$(mktemp -d "$WORK/bare.XXXX")"
    mkdir -p "$dir/scripts"
    cp "$ROOT/scripts/android-sdk-version.sh" "$ROOT/scripts/android-sdk-changelog.sh" \
        "$dir/scripts/"
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

published() { printf '%s\n' "$1" | "$v" check-newer --published "${@:2}"; }
# The same two helpers without the notes the scripts print on stderr for lines they ignore.
newer_quiet() { newer "$@" 2>/dev/null; }
published_quiet() { published "$@" 2>/dev/null; }
registry=$'0.0.0\n0.0.0-canary-0123abc\n0.0.0-om-rn1-4567def'
expect_ok "check-newer --published: canary builds count as their release" \
    "0.1.0-beta.1 is newer than the latest release, 0.0.0" -- published "$registry" 0.1.0-beta.1
expect_fail "check-newer --published: the release itself is taken" "already released" -- \
    published "$registry" 0.0.0
expect_fail "check-newer --published: a beta is not newer than its canary builds" \
    "counted as 0.0.0" -- published $'0.0.0-canary-0123abc\n0.0.0-om-rn1-4567def' 0.0.0-beta.1
expect_fail "check-newer --published: a release is not newer than a prerelease it counts as" \
    "0.0.0 is not newer than 0.0.0-canary-0123abc, which is already released and counts as 0.0.0" \
    -- \
    published "0.0.0-canary-0123abc" 0.0.0
expect_fail "check-newer --published: a prerelease outside the grammar still counts" \
    "1.0.0-rc.1 (counted as 1.0.0)" -- published $'1.0.0-rc.1\n0.0.0' 0.9.0
expect_ok "check-newer without --published ignores what is not in the grammar" \
    "0.9.0 is newer than the latest release, 0.0.0" -- newer_quiet $'1.0.0-rc.1\n0.0.0' 0.9.0
expect_ok "check-newer --published: a grammar beta is not rewritten" \
    "0.1.0 is newer than the latest release, 0.1.0-beta.3" -- published "0.1.0-beta.3" 0.1.0
expect_fail "check-newer --published: build metadata does not make a release newer" \
    "0.1.0+build.5" -- published "0.1.0+build.5" 0.1.0
expect_ok "check-newer --published: above build metadata" \
    "0.1.1-beta.1 is newer than the latest release, 0.1.0+build.5 (counted as 0.1.0)" -- \
    published "0.1.0+build.5" 0.1.1-beta.1
expect_ok "check-newer --published ignores what is not a version" \
    "0.1.0-beta.2 is newer than the latest release, 0.1.0-beta.1" -- \
    published_quiet $'foo\n0.1.0-beta.1' 0.1.0-beta.2
expect_ok "check-newer --published says it ignored a line" \
    $'ignoring foo: not a version\n0.1.0-beta.2 is newer than the latest release, 0.1.0-beta.1' -- \
    published $'foo\n0.1.0-beta.1' 0.1.0-beta.2
expect_ok "check-newer --published says nothing about a line it counts" \
    "0.1.0-beta.1 is newer than the latest release, 0.0.0-canary-1 (counted as 0.0.0)" -- \
    published "0.0.0-canary-1" 0.1.0-beta.1
expect_ok "check-newer --published: components with leading zeros are not versions" \
    "0.1.0 is newer than the latest release, 0.0.1" -- published_quiet $'01.0.0-rc.1\n0.0.1' 0.1.0
expect_ok "check-newer --published --except leaves the version's own entry out" \
    "0.1.0-beta.2 is newer than the latest release, 0.1.0-beta.1" -- \
    published $'0.1.0-beta.1\n0.1.0-beta.2' 0.1.0-beta.2 --except 0.1.0-beta.2
expect_fail "check-newer --published refuses a version outside the grammar" \
    "must be X.Y.Z-beta.N or X.Y.Z" -- published "0.1.0-beta.1" 0.1.0-rc.1
expect_ok "check-newer --published with nothing released" \
    "0.1.0-beta.1: nothing is released yet" -- published "" 0.1.0-beta.1
expect_usage "check-newer --published needs a version" -- published "0.1.0"

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

# `commits` needs neither the version catalog nor the changelog.
r="$(bare_repo)"
c="$r/scripts/android-sdk-changelog.sh"
git -C "$r" tag -a react-native-sdk-v0.1.0-beta.1 -m "React Native SDK 0.1.0-beta.1"
change "$r" js/react-native/react-native/src/index.ts "feat(rn): a new call"
first="$(git -C "$r" rev-parse HEAD)"
change "$r" rust/fedimint-sdk/src/lib.rs "fix(sdk): a bug"
change "$r" android/fedimint-sdk/build.gradle.kts "chore(android): unrelated to React Native"
change "$r" js/web/index.ts "feat(web): unrelated to React Native"
commits=("$c" commits --tag-prefix react-native-sdk-v --since 0.1.0-beta.1)
out="$("${commits[@]}" js/react-native rust/fedimint-sdk)"
for subject in "feat(rn): a new call" "fix(sdk): a bug"; do
    if [[ "$out" == *"- $subject ("* ]]; then
        pass "commits lists: $subject"
    else
        fail "commits lists: $subject" "$out"
    fi
done
for subject in "unrelated to React Native" "base"; do
    if [[ "$out" != *"$subject"* ]]; then
        pass "commits leaves out: $subject"
    else
        fail "commits leaves out: $subject" "$out"
    fi
done
bad="$(grep -Evc '^- .* \([0-9a-f]{7,}\)$' <<<"$out" || true)"
if [[ "$bad" -eq 0 && "$(wc -l <<<"$out")" -eq 2 ]]; then
    pass "commits prints '- <subject> (<hash>)' lines"
else
    fail "commits prints '- <subject> (<hash>)' lines" "$out"
fi
if [[ "$(head -n1 <<<"$out")" == "- fix(sdk): a bug ("* ]]; then
    pass "commits lists the newest commit first"
else
    fail "commits lists the newest commit first" "$out"
fi
expect_ok "commits --to stops at the given commit" \
    "- feat(rn): a new call ($(git -C "$r" rev-parse --short "$first"))" -- \
    "${commits[@]}" --to "$first" js/react-native rust/fedimint-sdk
expect_ok "commits takes options in any order" \
    "- feat(rn): a new call ($(git -C "$r" rev-parse --short "$first"))" -- \
    "$c" commits --to "$first" --since 0.1.0-beta.1 --tag-prefix react-native-sdk-v js/react-native
expect_ok "commits prints nothing when no commit qualifies" "" -- "${commits[@]}" docs
expect_fail "commits refuses an unknown commit" "no commit nonexistent in this checkout" -- \
    "${commits[@]}" --to nonexistent js
expect_fail "commits needs the tag with that prefix" \
    "no tag react-native-sdk-v0.9.9 in this checkout; fetch the tags and history" -- \
    "$c" commits --tag-prefix react-native-sdk-v --since 0.9.9 js
expect_usage "commits needs --since" -- "$c" commits --tag-prefix react-native-sdk-v js
expect_usage "commits needs --tag-prefix" -- "$c" commits --since 0.1.0-beta.1 js
expect_usage "commits needs a path" -- "${commits[@]}"
expect_usage "commits refuses an unknown option" -- "${commits[@]}" --sideways js
expect_usage "commits needs a value for --to" -- "${commits[@]}" --to

git -C "$r" checkout -q -b side "$first"
change "$r" js/react-native/react-native/src/side.ts "feat(rn): on a side branch"
git -C "$r" checkout -q main
git -C "$r" merge -q --no-ff side -m "Merge branch side"
out="$("${commits[@]}" js/react-native)"
if [[ "$out" == *"on a side branch"* && "$out" != *"Merge branch"* ]]; then
    pass "commits lists a merged commit but not the merge"
else
    fail "commits lists a merged commit but not the merge" "$out"
fi

r="$(bare_repo)"
c="$r/scripts/android-sdk-changelog.sh"
git -C "$r" tag android-sdk-v0.1.0-beta.1
change "$r" js/web/index.ts "feat(web): something"
expect_fail "commits does not see another prefix's tags" \
    "no tag react-native-sdk-v0.1.0-beta.1" -- \
    "$c" commits --tag-prefix react-native-sdk-v --since 0.1.0-beta.1 js
expect_ok "commits works from a lightweight tag" \
    "- feat(web): something ($(git -C "$r" rev-parse --short HEAD))" -- \
    "$c" commits --tag-prefix android-sdk-v --since 0.1.0-beta.1 js
for sub in "section 0.1.0-beta.1" "check 0.1.0-beta.1" "draft 0.1.0-beta.2 0.1.0-beta.1"; do
    # shellcheck disable=SC2086 # $sub is meant to split into the subcommand and its arguments.
    expect_fail "${sub%% *} needs android/CHANGELOG.md" "android/CHANGELOG.md" -- "$c" $sub
done

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
