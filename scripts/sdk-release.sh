#!/usr/bin/env bash
#
# Holds the rules every release of the SDK must pass, so that the workflows that release it and a
# maintainer's shell apply the same ones.
#
#   scripts/sdk-release.sh latest <component>
#   scripts/sdk-release.sh check-version <version>
#   scripts/sdk-release.sh check <component> [--except-own]
#   scripts/sdk-release.sh check-tag <tag>
#   scripts/sdk-release.sh check-tree
#   scripts/sdk-release.sh pending <commit>
#   scripts/sdk-release.sh untagged <component>...
#
# This repository releases three components, which are built from one Rust crate and defined in
# sdk-components.sh: `rust`, the `fedimint-sdk` crate on crates.io, `android`, the Android SDK on
# Maven Central, and `react-native`, the packages `@fedimint/react-native-bindings` and
# `@fedimint/react-native` on npm. Each has its own version in the tree, its own CHANGELOG.md, its
# own release tags, named `<prefix><version>`, and its own release workflow. A release starts
# when a pull request that changes the versions in the tree is merged: a workflow then tags each
# component whose version changed, and the tag starts that component's release. A pushed tag
# cannot be moved or deleted and no registry accepts a version twice, so a wrong release is
# permanent. These are the rules that prevent one:
#
#   1. A version is `X.Y.Z-beta.N` or `X.Y.Z`, and nothing else (see sdk-version.sh).
#   2. The components share one line of versions, whose version is the newest version any
#      component has in the tree. A release takes the next version of the line and gives it to
#      the components it publishes: all of them, or only some, such as a point release of one
#      binding or of both.
#   3. A new version is newer than every release tag of every component and than every version
#      already on a registry (either npm package, the crate), where a snapshot such as
#      `0.0.0-canary-abc123` counts as the release it starts with. A version is used once.
#   4. Components released together carry the same version and are tagged on the same commit. A
#      version is never tagged on two different commits.
#   5. In the committed tree every component is either at a released version, which has its
#      release tag, or at the line's version, which is the next one to be released.
#
# `latest` prints the newest released version of a component, taken from its release tags, or
# nothing when it has none. The version bump uses it to find the release that the draft of the
# next release notes starts from.
#
# `check-version` is rule 3 for a version that is only proposed: it fails unless the version is
# newer than every release tag of every component and than every version on a registry. A version
# bump runs it before proposing a version.
#
# `check` is what the tag workflow runs for a component before it tags it, and what a release
# workflow runs in its dry run, which publishes nothing. It fails unless the version the component
# has in the tree can be released from the commit that is checked out: it is newer than the
# component's own release tags and registry versions (rule 3), its release notes are finished (see
# sdk-changelog.sh) and no component has tagged the same version on another commit (rule 4). It
# ends with `<component> <version> can be released`. --except-own leaves that version itself out of
# the comparison with the tags and the registry, so that a release which stopped half way, after a
# tag or a registry took the version, can run again. Rule 4 applies to the component's own tag of
# that version as well.
#
# `check-tag` is what a release workflow runs first, for the tag that started it. The tag must be
# a release tag that exists here, with the commit it points at checked out, and the rest of its
# name must be the version the component has in the tree. That commit must also be on origin/main,
# because releases come from main. Then `check --except-own` must pass. A tag need not be
# annotated: the release notes are in the CHANGELOG.md. It ends with `<tag> can be released`.
#
# `check-tree` is for the pull request check and asks no registry. It requires of every
# component a valid version, finished release notes and rule 5: either the release tag of its
# version exists (it prints `released`), or the version is the line's and newer than every
# release tag of every component (it prints `to be released`). It reports every problem it
# finds, not only the first.
#
# `pending` is for the workflow that tags releases after a push to main. It prints, in the order
# of the components, each one whose version in the tree differs from its version at <commit>,
# the commit before the push, when the new version has no release tag yet and the old one has:
# a version bump after a release. For a version that changed from one that was never released
# this is not so, and it prints nothing but a note on stderr that the tag workflow can be run
# by hand to release it. It also notes on stderr a new version that is tagged already, and says
# nothing of a component that did not change.
#
# `untagged` is the same filter for a run of the tag workflow by hand, which names the components
# to tag: it prints those with no release tag for their version, and says on stderr that the tag
# exists for the others, so that running it again after it stopped half way tags what is missing.
#
# The script reads only local refs: fetch the tags and origin/main first (`git fetch --tags
# origin`, `git fetch origin main`). The versions on a registry are asked of the registry itself:
# of npm with `npm view`, and of the sparse index of crates.io with `curl`. The Android SDK's
# registry is not asked: its release tags are the record of its releases.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/sdk-components.sh
. "$ROOT/scripts/sdk-components.sh"
VERSION_SCRIPT="$ROOT/scripts/sdk-version.sh"
CHANGELOG_SCRIPT="$ROOT/scripts/sdk-changelog.sh"
# Releases come from main only.
MAIN_REF=origin/main
# The npm packages of the React Native component, the bindings first: the order they are
# published in.
NPM_PACKAGES=(@fedimint/react-native-bindings @fedimint/react-native)
# The crate of the Rust component and the page of its versions in the sparse index of crates.io.
CRATE=fedimint-sdk
CRATE_INDEX_URL=https://index.crates.io/fe/di/fedimint-sdk
# crates.io asks every client to say who it is.
USER_AGENT='fedimint-sdk release check (https://github.com/fedimint/fedimint-sdk)'
# The sparse index of crates.io answers a request for a crate with one JSON object per published
# version of the crate, one per line, the yanked versions included. An object describes its version
# with members such as the checksum, the dependencies and the yanked flag, of which only "vers",
# the version itself, is read. This matches that member together with its value.
VERS_RE='"vers"[[:space:]]*:[[:space:]]*"[^"]*"'

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Prints the versions of the release tags of the component $1, without the prefix, one per line.
released_versions() {
    local prefix tag
    prefix="$(component_tag_prefix "$1")" || exit 1
    git -C "$ROOT" tag -l "$prefix*" | while IFS= read -r tag; do
        echo "${tag#"$prefix"}"
    done
}

# Prints the registries that list the versions of the component $1, one per line, each by the
# name that the messages call it: an npm package by its name, the crate by `crates.io`. The
# Android SDK has none: its tags are the only record of its releases.
registries_of() {
    case "$1" in
        rust) echo crates.io ;;
        android) ;;
        react-native) printf '%s\n' "${NPM_PACKAGES[@]}" ;;
    esac
}

# Prints every version of the npm package $1, one per line, or nothing when it was never
# published. Any other failure of npm stops the script: a registry that cannot be reached must
# not look like a package with no versions, or any version would pass. npm, like curl below, gets
# /dev/null for its standard input: a tool that read the caller's would take what the caller still
# has to read from it, such as the names of the registries that follow in check_registries.
npm_versions() {
    local package="$1" out status=0 code
    out="$(npm view "$package" versions --json </dev/null)" || status=$?
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

# Prints every version of the crate on crates.io, one per line, the yanked ones included: a
# yanked version stays taken. A crate that was never published, which the index answers with a
# 404, has no versions. Any other failure stops the script, for the reason npm_versions gives.
# The index holds one JSON object per line, each with a "vers" member.
crate_versions() {
    local file code status=0 body versions stray
    file="$(mktemp)"
    code="$(curl --silent --show-error --max-time 60 --user-agent "$USER_AGENT" \
        --output "$file" --write-out '%{http_code}' "$CRATE_INDEX_URL" </dev/null)" || status=$?
    body="$(<"$file")"
    rm -f "$file"
    ((status == 0)) ||
        die "cannot list the versions of $CRATE on crates.io: curl exited with status $status;" \
            "check that curl works and that the network is reachable, then retry later"
    case "$code" in
        404) return 0 ;;
        200) ;;
        *)
            die "cannot list the versions of $CRATE on crates.io: $CRATE_INDEX_URL answered" \
                "HTTP $code; retry later"
            ;;
    esac
    # A listed crate has at least one version, and every line of its page has one.
    versions="$(printf '%s\n' "$body" | grep -o "$VERS_RE" |
        sed 's/^[^:]*:[[:space:]]*"\(.*\)"$/\1/' || true)"
    stray="$(printf '%s\n' "$body" | grep -v "$VERS_RE" | grep '[^[:space:]]' || true)"
    if [[ -z "$versions" || -n "$stray" ]]; then
        die "cannot list the versions of $CRATE on crates.io: the answer of $CRATE_INDEX_URL has" \
            "no versions, or a line without \"vers\"; retry later"
    fi
    printf '%s\n' "$versions"
}

# Prints every version that the registry $1, as registries_of names it, lists.
registry_versions() {
    case "$1" in
        crates.io) crate_versions ;;
        *) npm_versions "$1" ;;
    esac
}

# Prints the lines of stdin, each starting with "$1: ".
prefixed() {
    local label="$1" line
    while IFS= read -r line; do
        printf '%s: %s\n' "$label" "$line"
    done
}

# check_list <label> <versions> <version> <except> [--published]: runs `check-newer` for the
# version on the released versions, one per line, leaving out the version <except> (none when
# empty), and prints what it says with the label that names the list. When the list holds nothing
# but <except>, it says that <except> is there and why that is accepted, rather than that nothing
# is released. When the version is refused it prints that on stderr and fails.
check_list() {
    local label="$1" versions="$2" version="$3" except="$4" out status=0
    shift 4
    out="$("$VERSION_SCRIPT" check-newer "$@" "$version" --except "$except" \
        <<<"$versions" 2>&1)" || status=$?
    if ((status)); then
        prefixed "$label" <<<"$out" >&2
        return 1
    fi
    if [[ -n "$except" && "${out##*$'\n'}" == "$version: nothing is released yet" ]] &&
        grep -qxF -- "$except" <<<"$versions"; then
        out="${out%"$version: nothing is released yet"}$except is there already and counts as"
        out="$out this release's own, so that a release that stopped half way can run again"
    fi
    prefixed "$label" <<<"$out"
}

# Fails unless the version $2 is newer than every release tag of the component $1, leaving out the
# version $3 (none when empty).
check_tags() {
    local versions
    versions="$(released_versions "$1")" || exit 1
    check_list "$1 tags" "$versions" "$2" "${3:-}"
}

# Fails unless the version $2 is newer than every version that the registries of the component $1
# list, leaving out the version $3 (none when empty). The lists are read into variables first, so
# that a failure to read one stops the script rather than disappearing into a pipe.
check_registries() {
    local registry versions
    while IFS= read -r registry; do
        versions="$(registry_versions "$registry")" || exit 1
        check_list "$registry" "$versions" "$2" "${3:-}" --published || return 1
    done < <(registries_of "$1")
}

# Prints the newest released version of the component $1, or nothing when it has none.
latest() {
    local newest
    is_component "$1" || exit 1
    newest="$(released_versions "$1" | "$VERSION_SCRIPT" latest)"
    [[ -z "$newest" ]] || echo "$newest"
}

# Fails unless the version $1 is newer than every release tag and every published version of every
# component.
check_version() {
    local component
    for component in "${COMPONENTS[@]}"; do
        check_tags "$component" "$1" || exit 1
        check_registries "$component" "$1" || exit 1
    done
}

# Succeeds if the tag $1 exists in this checkout. Only a tag counts: a branch or any other ref of
# the same name does not.
has_tag() {
    git -C "$ROOT" rev-parse -q --verify "refs/tags/$1" >/dev/null
}

# Fails unless every release tag of the version $2 that exists, that of the component $1 itself
# and those of the other components, names the checked out commit: a version is never tagged on
# two commits. A tag that exists but does not name a commit fails as well.
need_same_commit() {
    local component="$1" version="$2" head other tag commit
    head="$(git -C "$ROOT" rev-parse HEAD)" || exit 1
    for other in "${COMPONENTS[@]}"; do
        tag="$(component_tag_prefix "$other")$version"
        has_tag "$tag" || continue
        commit="$(git -C "$ROOT" rev-parse -q --verify "refs/tags/$tag^{commit}" 2>/dev/null)" ||
            die "$tag ($other) does not point at a commit:" \
                "a release tag names the commit it releases"
        [[ "$commit" == "$head" ]] ||
            die "$tag ($other) is on commit $(git -C "$ROOT" rev-parse --short "$commit")," \
                "not on this commit, $(git -C "$ROOT" rev-parse --short "$head"):" \
                "$version is taken by that commit, so $component needs the next version"
    done
}

# Fails unless the version $2 of the component $1 can be released from the checked out commit,
# leaving out the version $3 (none when empty) from the versions it must be newer than.
check_release() {
    local component="$1" version="$2" except="${3:-}"
    check_tags "$component" "$version" "$except" || exit 1
    check_registries "$component" "$version" "$except" || exit 1
    "$CHANGELOG_SCRIPT" check "$component" "$version" || exit 1
    need_same_commit "$component" "$version"
}

# Fails unless the version that the component $1 has in the tree can be released from the checked
# out commit. With $2 set to --except-own that version itself is not counted against it.
check() {
    local component="$1" version except=""
    is_component "$component" || exit 1
    version="$("$VERSION_SCRIPT" current "$component")" || exit 1
    [[ "${2:-}" != --except-own ]] || except="$version"
    check_release "$component" "$version" "$except"
    echo "$component $version can be released"
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

# Fails unless the release tag $1 can be released: it exists, it is on the commit that is checked
# out and on origin/main, it names the version that its component has in the tree, and that
# version passes `check` without counting the tag itself.
check_tag() {
    local tag="$1" component="" other prefix prefixes="" version commit head tree_version
    for other in "${COMPONENTS[@]}"; do
        prefix="$(component_tag_prefix "$other")"
        prefixes="${prefixes:+$prefixes, }$prefix"
        if [[ -z "$component" && "$tag" == "$prefix"* ]]; then
            component="$other"
            version="${tag#"$prefix"}"
        fi
    done
    [[ -n "$component" ]] ||
        die "$tag is not a release tag: it must start with one of $prefixes"
    commit="$(git -C "$ROOT" rev-parse -q --verify "refs/tags/$tag^{commit}")" ||
        die "no tag $tag in this checkout; fetch it first"
    head="$(git -C "$ROOT" rev-parse HEAD)" || exit 1
    [[ "$commit" == "$head" ]] ||
        die "$tag points at commit $(git -C "$ROOT" rev-parse --short "$commit"), but the" \
            "checked out commit is $(git -C "$ROOT" rev-parse --short "$head"):" \
            "check the tag out first"
    tree_version="$("$VERSION_SCRIPT" current "$component")" || exit 1
    [[ "$version" == "$tree_version" ]] ||
        die "$tag names version $version, but $component is at $tree_version in the tree:" \
            "a release tag must name the version it releases"
    need_on_main "$tag" "$commit"
    check_release "$component" "$tree_version" "$tree_version"
    echo "$tag can be released"
}

# What check_tree has found wrong with the tree so far.
PROBLEMS=0

# Reports a problem with the tree on stderr, and counts it.
problem() {
    echo "$*" >&2
    PROBLEMS=$((PROBLEMS + 1))
}

# Reports whether each component is at a version it released or at the one to be released next,
# and every problem with a version or with the notes of one, without asking any registry. Every
# component is checked, so that all the problems come out at once.
check_tree() {
    local component version line out last other refused readable=""
    # The line's version is the newest of the versions that could be read.
    for component in "${COMPONENTS[@]}"; do
        if out="$("$VERSION_SCRIPT" current "$component" 2>&1)"; then
            readable="${readable:+$readable$'\n'}$component $out"
        else
            problem "$component: $out"
        fi
    done
    line="$(cut -d' ' -f2 <<<"$readable" | "$VERSION_SCRIPT" latest)"
    while read -r component version; do
        [[ -n "$component" ]] || continue
        if has_tag "$(component_tag_prefix "$component")$version"; then
            echo "$component $version: released"
        elif [[ "$version" != "$line" ]]; then
            last="$(released_versions "$component" | "$VERSION_SCRIPT" latest 2>/dev/null)"
            problem "$component: $version has no release tag and is not the line's version," \
                "$line: set $component to $line${last:+, or back to $last, its last release}"
        else
            refused=0
            for other in "${COMPONENTS[@]}"; do
                out="$(check_tags "$other" "$version" 2>&1)" || {
                    problem "$component: $version has no release tag yet, but $out"
                    refused=1
                }
            done
            ((refused)) || echo "$component $version: to be released"
        fi
        if ! out="$("$CHANGELOG_SCRIPT" check "$component" "$version" 2>&1)"; then
            problem "$component: $out"
        fi
    done <<<"$readable"
    ((PROBLEMS == 0)) ||
        die "the committed versions and notes cannot be released as they are;" \
            "fix the problems above"
}

# Prints the components, in the order of COMPONENTS, whose version changed since the commit $1 from
# a released version to one that has no release tag yet. On stderr it says why it leaves out each
# of the other components whose version changed.
pending() {
    local commit="$1" component old new prefix result=""
    git -C "$ROOT" rev-parse -q --verify "$commit^{commit}" >/dev/null ||
        die "no commit $commit in this checkout"
    for component in "${COMPONENTS[@]}"; do
        new="$("$VERSION_SCRIPT" current "$component")" || exit 1
        old="$("$VERSION_SCRIPT" current "$component" --at "$commit")" || exit 1
        if [[ "$new" == "$old" ]]; then
            continue
        fi
        prefix="$(component_tag_prefix "$component")"
        if has_tag "$prefix$new"; then
            echo "$prefix$new already exists; nothing to release" >&2
        elif has_tag "$prefix$old"; then
            result="${result:+$result$'\n'}$component"
        else
            echo "$component: the version changed from $old, which was never released, to $new," \
                "so this is not a version bump after a release; the tag workflow can be run by" \
                "hand to release $new" >&2
        fi
    done
    [[ -z "$result" ]] || echo "$result"
}

# Prints those of the components $@ whose version has no release tag yet, in the order they are
# first named. On stderr it says that the tag exists for each of the others. A component named
# more than once counts once.
untagged() {
    local component version tag seen=" " result=""
    for component in "$@"; do
        is_component "$component" || exit 1
    done
    for component in "$@"; do
        [[ "$seen" != *" $component "* ]] || continue
        seen="$seen$component "
        version="$("$VERSION_SCRIPT" current "$component")" || exit 1
        tag="$(component_tag_prefix "$component")$version"
        if has_tag "$tag"; then
            echo "$tag already exists; nothing to release" >&2
        else
            result="${result:+$result$'\n'}$component"
        fi
    done
    [[ -z "$result" ]] || echo "$result"
}

case "${1:-}" in
    latest)
        [[ $# -eq 2 ]] || usage
        latest "$2"
        ;;
    check-version)
        [[ $# -eq 2 ]] || usage
        check_version "$2"
        ;;
    check)
        [[ $# -eq 2 || ($# -eq 3 && "$3" == --except-own) ]] || usage
        check "${@:2}"
        ;;
    check-tag)
        [[ $# -eq 2 ]] || usage
        check_tag "$2"
        ;;
    check-tree)
        [[ $# -eq 1 ]] || usage
        check_tree
        ;;
    pending)
        [[ $# -eq 2 && -n "$2" && "$2" != -* ]] || usage
        pending "$2"
        ;;
    untagged)
        [[ $# -ge 2 ]] || usage
        shift
        untagged "$@"
        ;;
    *) usage ;;
esac
