#!/usr/bin/env bash
#
# Decides what a valid React Native SDK release tag is, drafts its notes and creates it.
#
#   scripts/react-native-sdk-release.sh notes [<commit>]
#   scripts/react-native-sdk-release.sh tag <version> <notes file> [<commit>]
#   scripts/react-native-sdk-release.sh check <version>
#   scripts/react-native-sdk-release.sh check-tag <tag>
#   scripts/react-native-sdk-release.sh message <tag>
#
# `@fedimint/react-native` and `@fedimint/react-native-bindings` are released to npm by pushing
# an annotated tag `react-native-sdk-v<version>` to main. The tag is the version and its message
# is the release notes. A tag cannot be moved or deleted once it is pushed, and npm never
# accepts a version twice, so a wrong tag is permanent. This script holds every rule about what
# a release tag is, so the release workflow and a maintainer's shell apply the same ones.
#
# A version is `X.Y.Z-beta.N` or `X.Y.Z`, and nothing else (see android-sdk-version.sh). It must
# be newer than every earlier `react-native-sdk-v*` tag and than every version of either package
# on npm, where a snapshot such as `0.0.0-canary-abc123` counts as the release it starts with.
# A tag whose version is not in that grammar never released anything and is not counted.
#
# `check-tag` is what a release does first for a pushed tag: the tag starts with
# `react-native-sdk-v`, is annotated, has a message, points at a commit on origin/main, and its
# version passes the checks above. The tag itself is left out of those checks, and so is a
# version that an earlier attempt of the same release already published to npm, which is why
# re-running a release that stopped half way passes. `check` runs the version checks alone,
# for a dry run that has a version but no tag.
#
# `notes` lists the commits since the latest `react-native-sdk-v*` tag that touched what the two
# packages are built from, up to <commit> (HEAD when not given). It is a starting point: the
# notes are rewritten for users before they become a tag message.
#
# `tag` creates the annotated tag for <version> from a file holding the notes, on <commit>
# (origin/main when not given), after the same checks. It only creates the tag in this
# repository. Pushing it, which starts the release, is left to the person running it.
#
# `message` prints a tag's message, which is its release notes, as it was written: every line
# and blank line in place, without the signature of a signed tag. The release puts it on the
# GitHub Release.
#
# The script reads only local refs: fetch the tags and origin/main first
# (`git fetch --tags origin`, `git fetch origin main`). The versions published to npm are asked
# of npm itself. .github/workflows/react-native-sdk-release.yaml runs `check-tag`, or `check`
# for a dry run, as its first job.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION_SCRIPT="$ROOT/scripts/android-sdk-version.sh"
CHANGELOG_SCRIPT="$ROOT/scripts/android-sdk-changelog.sh"
TAG_PREFIX=react-native-sdk-v
# Bindings first: the order they are published in.
PACKAGES=(@fedimint/react-native-bindings @fedimint/react-native)
# Releases come from main only.
MAIN_REF=origin/main
# What the two published packages are built from. A commit that touches none of these cannot
# change them, so it does not belong in their notes.
PATHS=(
    js/react-native
    rust/fedimint-sdk
    rust/ubrn
    nix
    flake.nix
    flake.lock
    scripts/generate-sdk-rn-bindings.sh
    scripts/assemble-rn-ios-xcframework.sh
    scripts/react-native-sdk-pack.sh
)

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Prints the versions of the local release tags, without the prefix, one per line.
released_tags() {
    local tag
    git -C "$ROOT" tag -l "${TAG_PREFIX}*" | while IFS= read -r tag; do
        echo "${tag#"$TAG_PREFIX"}"
    done
}

# Prints every version of the npm package $1, one per line, or nothing when it was never
# published. Any other failure of npm stops the script: a registry that cannot be reached must
# not look like a package with no versions, or any version would pass.
published_versions() {
    local package="$1" out status=0 code
    out="$(npm view "$package" versions --json)" || status=$?
    if ((status == 0)); then
        # npm prints a JSON list, or a bare JSON string when there is exactly one version.
        printf '%s' "$out" | node -e '
            let versions = JSON.parse(require("fs").readFileSync(0, "utf8"));
            if (typeof versions === "string") versions = [versions];
            if (!Array.isArray(versions) || !versions.every((v) => typeof v === "string")) {
                process.exit(1);
            }
            versions.forEach((v) => console.log(v));
        ' 2>/dev/null ||
            die "npm listed the versions of $package in a form this script does not read: $out"
        return
    fi
    code="$(printf '%s' "$out" | node -e '
        const error = JSON.parse(require("fs").readFileSync(0, "utf8")).error;
        if (error && typeof error.code === "string") console.log(error.code);
    ' 2>/dev/null || true)"
    # A package that was never published has no versions to conflict with.
    [[ "$code" != E404 ]] || return 0
    if [[ -n "$code" ]]; then
        die "cannot list the versions of $package on npm: error code $code; retry later"
    fi
    die "cannot list the versions of $package on npm: npm exited with status $status and" \
        "printed ${out:-nothing}; check that npm works and retry"
}

# Prints the lines of stdin, each starting with "$1: ".
prefixed() {
    local label="$1" line
    while IFS= read -r line; do
        printf '%s: %s\n' "$label" "$line"
    done
}

# check_list <label> <versions> <check-newer arguments...>: runs `check-newer` on the released
# versions, one per line, and prints what it says with the label that names the list. It stops
# the script when the version is refused.
check_list() {
    local label="$1" versions="$2" out status=0
    shift 2
    out="$("$VERSION_SCRIPT" check-newer "$@" <<<"$versions" 2>&1)" || status=$?
    if ((status)); then
        prefixed "$label" <<<"$out" >&2
        exit 1
    fi
    prefixed "$label" <<<"$out"
}

# Fails unless $1 is newer than every release tag and every version of the packages on npm,
# leaving out the version $2 (none when empty). The lists are read into variables first, so a
# failure to read one stops the script rather than disappearing into a pipe. An empty --except
# leaves nothing out.
check_version() {
    local version="$1" except="${2:-}" package versions
    versions="$(released_tags)"
    check_list tags "$versions" "$version" --except "$except"
    for package in "${PACKAGES[@]}"; do
        versions="$(published_versions "$package")"
        check_list "$package" "$versions" --published "$version" --except "$except"
    done
}

# Fails unless the commit $2 is on origin/main. $1 is how the failure names it.
need_on_main() {
    local name="$1" commit="$2"
    git -C "$ROOT" rev-parse -q --verify "$MAIN_REF^{commit}" >/dev/null ||
        die "no $MAIN_REF in this checkout; fetch it first"
    git -C "$ROOT" merge-base --is-ancestor "$commit" "$MAIN_REF" ||
        die "$name (commit $(git -C "$ROOT" rev-parse --short "$commit")) is not on $MAIN_REF;" \
            "releases come from main, so merge it there and fetch first"
}

# Fails unless $1 is a release tag that exists here and is annotated. A lightweight tag has no
# message of its own: git would show the tagged commit's message in its place.
need_annotated() {
    local tag="$1" kind
    [[ "$tag" == "$TAG_PREFIX"* ]] || die "$tag is not a ${TAG_PREFIX}* tag"
    git -C "$ROOT" rev-parse -q --verify "refs/tags/$tag" >/dev/null ||
        die "no tag $tag in this checkout; fetch it first"
    kind="$(git -C "$ROOT" cat-file -t "refs/tags/$tag")"
    [[ "$kind" == tag ]] ||
        die "$tag is not an annotated tag: release tags must be annotated, because their" \
            "message is the release notes"
}

# Prints the message of the annotated tag $1 without the signature of a signed tag. The message
# is not read as subject and body: git joins the lines of a subject into one, which would run
# the first paragraph of the notes together.
tag_message() {
    local tag="$1" contents signature
    contents="$(git -C "$ROOT" for-each-ref --format='%(contents)' "refs/tags/$tag")"
    signature="$(git -C "$ROOT" for-each-ref --format='%(contents:signature)' "refs/tags/$tag")"
    printf '%s\n' "${contents%"$signature"}"
}

check_tag() {
    local tag="$1" message commit
    need_annotated "$tag"
    message="$(tag_message "$tag")"
    [[ -n "${message//[[:space:]]/}" ]] ||
        die "$tag has no release notes: its message is blank; the message is the release notes"
    commit="$(git -C "$ROOT" rev-parse "refs/tags/$tag^{commit}")"
    need_on_main "$tag" "$commit"
    check_version "${tag#"$TAG_PREFIX"}" "${tag#"$TAG_PREFIX"}"
    echo "$tag can be released"
}

notes() {
    local commit="${1:-HEAD}" previous listed
    previous="$(released_tags | "$VERSION_SCRIPT" latest)"
    if [[ -z "$previous" ]]; then
        echo "nothing is released yet, so there are no earlier commits to list;" \
            "write the notes for the first release by hand" >&2
        return
    fi
    listed="$("$CHANGELOG_SCRIPT" commits --tag-prefix "$TAG_PREFIX" --since "$previous" \
        --to "$commit" "${PATHS[@]}")"
    echo "these are the commits since $TAG_PREFIX$previous that touched the React Native" \
        "packages; rewrite them for users before tagging" >&2
    echo "${listed:-- No changes to the React Native packages since $previous.}"
}

tag_release() {
    local version="$1" file="$2" commit="${3:-$MAIN_REF}" tag full
    tag="$TAG_PREFIX$version"
    [[ -f "$file" ]] || die "no notes file $file; write the release notes there first"
    grep -q '[^[:space:]]' "$file" || die "$file has no release notes: it is blank"
    if git -C "$ROOT" rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        die "the tag $tag already exists: release tags are never moved, so pick a new version"
    fi
    full="$(git -C "$ROOT" rev-parse -q --verify "$commit^{commit}")" ||
        die "no commit $commit in this checkout; fetch it first"
    need_on_main "$commit" "$full"
    check_version "$version"
    # Git's default cleanup for a tag message drops every line starting with `#`, which would
    # delete the notes' Markdown headings, so only the whitespace is cleaned up. The notes are
    # read from stdin so that a relative path means the caller's directory, not the repository.
    git -C "$ROOT" tag -a --cleanup=whitespace -F - "$tag" "$full" <"$file"
    echo "tagged $tag at $(git -C "$ROOT" rev-parse --short "$full")"
    echo "To release it, run:"
    echo
    echo "    git push origin $tag"
    echo
    echo "Pushing the tag starts the release. The tag cannot be moved or deleted afterwards."
}

case "${1:-}" in
    notes)
        [[ $# -le 2 ]] || usage
        notes "${2:-}"
        ;;
    tag)
        [[ $# -eq 3 || $# -eq 4 ]] || usage
        tag_release "$2" "$3" "${4:-}"
        ;;
    check)
        [[ $# -eq 2 ]] || usage
        check_version "$2"
        ;;
    check-tag)
        [[ $# -eq 2 ]] || usage
        check_tag "$2"
        ;;
    message)
        [[ $# -eq 2 ]] || usage
        need_annotated "$2"
        tag_message "$2"
        ;;
    *) usage ;;
esac
