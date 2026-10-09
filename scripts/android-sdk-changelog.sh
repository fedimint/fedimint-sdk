#!/usr/bin/env bash
#
# Reads and drafts android/CHANGELOG.md, the Android SDK's release notes.
#
#   scripts/android-sdk-changelog.sh section <version>
#   scripts/android-sdk-changelog.sh check <version>
#   scripts/android-sdk-changelog.sh draft <version> [<previous version>]
#   scripts/android-sdk-changelog.sh commits --tag-prefix <prefix> --since <version> [--to <commit>] <path>...
#
# Each release has a `## <version>` section, newest first.
#
# `section` prints a version's notes, which the release workflow turns into
# that version's GitHub Release. `check` fails unless the section exists, has
# something in it, and is no longer a draft. The release refuses a version
# that fails it, so nothing is published without notes a person has read.
#
# `draft` adds a section for <version> above the others. It lists the commits
# since `android-sdk-v<previous version>` that touched what the SDK is built
# from, and it is marked as a draft. The bump workflow runs it for the version
# bump pull request. The notes are rewritten for users in that pull request,
# and the marker is removed there. It needs the git history back to that tag.
#
# `commits` prints the commit list `draft` starts from, for any SDK: the
# non-merge commits from the tag `<prefix><version>` up to <commit> (HEAD when
# not given) that touched at least one of the paths, newest first, as
# `- <subject> (<abbreviated hash>)`. It reads no file, so other SDK releases
# draft their notes from it without an android/CHANGELOG.md. It fails when the
# tag, annotated or lightweight, is not in the checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHANGELOG="$ROOT/android/CHANGELOG.md"
DRAFT_MARKER='<!-- draft:'
# What the published AAR is built from. A commit that touches none of these
# cannot change the SDK, so it does not belong in its notes. The directories
# include their Cargo lockfiles (rust/fedimint-sdk/Cargo.lock,
# rust/uniffi-bindgen/Cargo.lock), so a dependency-only bump is listed too;
# scripts/test-android-sdk-scripts.sh checks that.
SDK_PATHS=(
    android/fedimint-sdk
    android/gradle
    android/build.gradle.kts
    android/settings.gradle.kts
    android/gradle.properties
    rust/fedimint-sdk
    rust/uniffi-bindgen
    nix
    flake.nix
    flake.lock
    scripts/generate-android-bindings.sh
    scripts/nix-build-android-so.sh
)
# The bump workflow's own commits, which only change the version and these
# notes. Matched after the `- ` each listed commit starts with, so no `^` here.
BUMP_SUBJECT='chore\(android\): bump the Android SDK to '

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Fails unless android/CHANGELOG.md exists, which `section`, `check` and `draft`
# need.
need_changelog() {
    [[ -f "$CHANGELOG" ]] || die "no $CHANGELOG"
}

# Prints the lines under `## $1` up to the next `## ` heading, without the
# blank lines around them. Fails when there is no such section.
section() {
    local version="$1"
    grep -qxF "## $version" "$CHANGELOG" || return 1
    awk -v heading="## $version" '
        $0 == heading { inside = 1; next }
        inside && /^## / { exit }
        inside { lines[++n] = $0 }
        END {
            first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
            last = n;  while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
            for (i = first; i <= last; i++) print lines[i]
        }
    ' "$CHANGELOG"
}

# commits --tag-prefix <prefix> --since <version> [--to <commit>] <path>...
# Prints "- <subject> (<hash>)" for each non-merge commit in
# `<prefix><version>..<commit>` that touched one of the paths, newest first.
commits() {
    local prefix="" since="" to=HEAD tag target
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --tag-prefix | --since | --to)
                [[ $# -ge 2 ]] || usage
                case "$1" in
                    --tag-prefix) prefix="$2" ;;
                    --since) since="$2" ;;
                    --to) to="$2" ;;
                esac
                shift 2
                ;;
            -*) usage ;;
            *) break ;;
        esac
    done
    [[ -n "$prefix" && -n "$since" && $# -gt 0 ]] || usage
    tag="refs/tags/$prefix$since"
    git -C "$ROOT" rev-parse -q --verify "$tag" >/dev/null ||
        die "no tag $prefix$since in this checkout; fetch the tags and history"
    target="$(git -C "$ROOT" rev-parse -q --verify "$to^{commit}")" ||
        die "no commit $to in this checkout"
    git -C "$ROOT" log --no-merges --format='- %s (%h)' "$tag..$target" -- "$@"
}

check() {
    local version="$1" notes
    notes="$(section "$version")" ||
        die "android/CHANGELOG.md has no \`## $version\` section; add the notes for $version"
    [[ -n "$notes" ]] || die "android/CHANGELOG.md's \`## $version\` section is empty"
    if grep -qF "$DRAFT_MARKER" <<<"$notes"; then
        die "android/CHANGELOG.md's \`## $version\` section is still a draft: rewrite it and remove the draft comment"
    fi
    echo "android/CHANGELOG.md has notes for $version"
}

draft() {
    local version="$1" previous="${2:-}" listed commits entry tmp
    if grep -qxF "## $version" "$CHANGELOG"; then
        die "android/CHANGELOG.md already has a \`## $version\` section"
    fi
    if [[ -n "$previous" ]]; then
        listed="$(commits --tag-prefix android-sdk-v --since "$previous" "${SDK_PATHS[@]}")"
        commits="$(grep -Ev -- "^- $BUMP_SUBJECT" <<<"$listed" || true)"
        [[ -n "$commits" ]] || commits="- No changes to the SDK since $previous."
        entry="$DRAFT_MARKER the commits since $previous that touched the SDK.
     Rewrite them for users: what was added, changed or fixed, and anything that
     breaks callers. Then delete this comment; the release refuses a draft. -->

$commits"
    else
        entry="$DRAFT_MARKER nothing is released yet, so there are no earlier
     commits to list. Write the notes for the first release, then delete this
     comment; the release refuses a draft. -->"
    fi
    # The new section goes above the newest one, or at the end if there is none.
    # The entry goes through the environment rather than `awk -v`, which would
    # read backslashes in commit subjects as escapes and, in BSD awk, refuses
    # a value with newlines.
    tmp="$(mktemp)"
    ENTRY="$entry" awk -v heading="## $version" '
        BEGIN { entry = ENVIRON["ENTRY"] }
        !done && /^## / { print heading; print ""; print entry; print ""; done = 1 }
        { print }
        END { if (!done) { print ""; print heading; print ""; print entry } }
    ' "$CHANGELOG" >"$tmp"
    mv "$tmp" "$CHANGELOG"
}

case "${1:-}" in
    section)
        need_changelog
        [[ $# -eq 2 ]] || usage
        section "$2" || die "android/CHANGELOG.md has no \`## $2\` section"
        ;;
    check)
        need_changelog
        [[ $# -eq 2 ]] || usage
        check "$2"
        ;;
    draft)
        need_changelog
        [[ $# -eq 2 || $# -eq 3 ]] || usage
        draft "$2" "${3:-}"
        ;;
    commits)
        shift
        commits "$@"
        ;;
    *) usage ;;
esac
