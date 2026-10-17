#!/usr/bin/env bash
#
# Holds the rules every release of the SDK must pass, so that the workflows that release it and a
# maintainer's shell apply the same ones, and does what the version bump and tag workflows do to
# the tree and the repository, so that those hold no logic of their own.
#
#   scripts/sdk-release.sh latest <component>
#   scripts/sdk-release.sh published <component>
#   scripts/sdk-release.sh check-version <version>
#   scripts/sdk-release.sh check <component> [--except-own]
#   scripts/sdk-release.sh check-tag <tag>
#   scripts/sdk-release.sh check-tree
#   scripts/sdk-release.sh pending <commit>
#   scripts/sdk-release.sh untagged <component>...
#   scripts/sdk-release.sh bump <all|bindings|component> <major|minor|patch> [--beta]
#   scripts/sdk-release.sh bump <all|bindings|component> <beta|release>
#   scripts/sdk-release.sh tag <component>...
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
# `published` prints the versions that the registries of a component list, one per line: those of
# the crate on crates.io for `rust`, the yanked ones included, and those of both npm packages for
# `react-native`, the bindings first and each version once. It prints nothing for `android`, which
# has no registry, and for a package that was never published. These are the versions that
# `check` and `check-version` compare with, for a script that has to know whether one is taken,
# such as rust-sdk-publish.sh. A registry that cannot be asked fails the command, and nothing is
# printed, so that a list which stops short is not taken for the whole.
#
# `check-version` is rule 3 for a version that is only proposed: it fails unless the version is
# newer than every release tag of every component and than every version on a registry. A version
# bump runs it before proposing a version.
#
# `check` is what the tag workflow runs for a component before it tags it, and what a release
# workflow runs in its dry run, which publishes nothing. It fails unless the version the component
# has in the tree can be released from the commit that is checked out: it is newer than the release
# tags of every component and than the versions on the registries the component is published to
# (rule 3), its release notes are finished (see sdk-changelog.sh) and no component has tagged the
# same version on another commit (rule 4). So a version below the release tag of another component
# is refused, whichever component it is for. The registries of the other components are not asked,
# as their release tags are the record of their releases, and a tag of another component for this
# very version is no obstacle: rule 4 holds it to the checked out commit, which is how components
# released together are tagged. It reads the working tree, so that it also tells someone about to
# commit a version whether it could be released; `tag` and `check-tag`, which release a commit,
# also require that what they read is committed. It ends with `<component> <version> can be
# released`. --except-own leaves that version itself out of the comparison with the component's own
# tags and registry versions, so that a release which stopped half way, after a tag or a registry
# took the version, can run again. The tags of the other components are compared the same way with
# and without it. Rule 4 applies to the component's own tag of that version as well.
#
# `check-tag` is what a release workflow runs first, for the tag that started it. The tag must be
# a release tag that exists here, with the commit it points at checked out, and the rest of its
# name must be the version the component has in the tree. The files that hold that version and the
# component's CHANGELOG.md must have no change that is not committed: the checks read the working
# tree, but what a tag releases is the commit. That commit must also be on origin/main, because
# releases come from main. Then `check --except-own` must pass: the tag itself does not stand in
# the way of its version, but a higher release tag of any component does. A tag need not be
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
# this is not so, and the same holds for one that cannot be read at <commit> (it is missing there,
# or outside the grammar, or the React Native manifests disagree): nothing is released by
# surprise, so it prints nothing for that component but a note on stderr. The note says that the
# tag workflow has to be run by hand for it now, before anything else is merged, because
# components released together are tagged on one commit. It also notes on stderr a new version
# that is tagged already, and says nothing of a component that did not change. A <commit> that
# is not in the checkout fails, and so does a version that cannot be read in the tree.
#
# `untagged` is the same filter for a run of the tag workflow by hand, which names the components
# to tag: it prints those with no release tag for their version, and says on stderr that the tag
# exists for the others, so that running it again after it stopped half way tags what is missing.
#
# `bump` is what the version bump workflow does to the tree, without git. Its first argument says
# which components get the new version: `all` of them, the `bindings` or one component, as
# `sdk-version.sh components` knows them. The others are what `sdk-version.sh next` takes. Its
# answer, the next version of the line, is the new version of every component named, whatever
# version each has: one that is behind the line is brought to the line's next, not to its own.
# `bump` refuses what `next` refuses, such as `patch` in the middle of a beta, and then runs
# `check-version` on the new version. For each component named it looks up the latest release
# (`latest`) and checks that its notes can take a new section. Only then does it write: the new
# version into the files that hold it (`sdk-version.sh set`), and into the notes of each component a
# draft of the new version's section (`sdk-changelog.sh draft`) that lists the commits since its
# latest release, or has the form that says nothing is released yet when it has none. Nothing is
# written unless every check before the first write passed, and the version files and notes of the
# components that were not named are not touched. The progress goes to stderr, and the new version
# alone to stdout.
#
# `tag` is what the tag workflow does to the repository, without pushing. It takes the components
# to tag, at least one. What is tagged is the commit, but the checks and the notes of the tags are
# read from the working tree, so for every component it first requires that the files that hold
# its version, and its CHANGELOG.md, have no change that is not committed (`sdk-version.sh files`
# names the first). Then it runs `check` for every one of them. Only if all pass does it create,
# at HEAD, an annotated tag for each: `<prefix><version>`, with the component's tag prefix and the
# version it has in the tree. The message of a tag is `<title> <version>`, such as `Android SDK
# 0.2.0`, as its first paragraph and the notes of that version (`sdk-changelog.sh section`) as the
# rest. The notes are kept as they are: git's default cleanup of a tag message would drop every
# line that starts with `#`, such as a heading. If a check fails or a tag exists already, no tag is
# created, and if git still cannot create one, the tags created before it are deleted again. The
# checks are reported on stderr and the tags created on stdout, one per line, in the order the
# components were named. A component named more than once counts once. The tagger is whoever git is
# configured for. `tag` never pushes: that, and starting the releases, is up to the caller.
#
# The script reads only local refs: fetch the tags and origin/main first (`git fetch --tags
# origin`, `git fetch origin main`). The versions on a registry are asked of the registry itself:
# of npm with `npm view`, and of the sparse index of crates.io with `curl`. The Android SDK's
# registry is not asked: its release tags are the record of its releases. The script writes
# nothing but what `bump` writes to the working tree and what `tag` writes as local tags.
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

# Prints every version that the registries of the component $1 list, one per line, each once: the
# versions of the registries in the order registries_of gives them, and those of a registry in the
# order it lists them. Every list is read before anything is printed, so that a registry that
# cannot be asked stops the script with nothing printed, rather than with the lists before it.
published() {
    local registry versions all=""
    is_component "$1" || exit 1
    while IFS= read -r registry; do
        versions="$(registry_versions "$registry")" || exit 1
        [[ -z "$versions" ]] || all="${all:+$all$'\n'}$versions"
    done < <(registries_of "$1")
    [[ -z "$all" ]] || awk '!seen[$0]++' <<<"$all"
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

# Fails unless the version $2 of the component $1 can be released from the checked out commit. It
# must be newer than the release tags of every component, so that a release never goes out below
# one that is out already, and than the versions on the registries of $1. The version $3 (none
# when empty) is left out of the comparison with the tags and the registries of $1. A tag of
# another component for the version $2 itself is always left out, as need_same_commit holds it to
# this commit, and the registries of the other components are not asked: their tags are the record
# of their releases. Only a refusal is printed for the other components.
check_release() {
    local component="$1" version="$2" except="${3:-}" other
    check_tags "$component" "$version" "$except" || exit 1
    for other in "${COMPONENTS[@]}"; do
        [[ "$other" != "$component" ]] || continue
        check_tags "$other" "$version" "$version" >/dev/null || exit 1
    done
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

# Fails unless the files that hold the version of the component $1, and its release notes, are in
# the working tree as they are in the checked out commit. A release tag names the commit, but the
# checks and the notes of the tag are read from the working tree: a change that is not committed
# would be checked, and tagged, as if the commit had it.
need_committed() {
    local component="$1" names file changelog modified untracked changed=""
    local -a files=()
    names="$("$VERSION_SCRIPT" files "$component")" || exit 1
    while IFS= read -r file; do
        files+=("$file")
    done <<<"$names"
    changelog="$(component_changelog "$component")" || exit 1
    files+=("$changelog")
    modified="$(git -C "$ROOT" diff --name-only HEAD -- "${files[@]}")" || exit 1
    untracked="$(git -C "$ROOT" ls-files --others -- "${files[@]}")" || exit 1
    while IFS= read -r file; do
        [[ -z "$file" ]] || changed="${changed:+$changed, }$file"
    done <<<"$modified"$'\n'"$untracked"
    [[ -z "$changed" ]] ||
        die "$component has changes that are not committed, in $changed:" \
            "what is tagged is the commit, not the working tree," \
            "so they have to be committed first"
}

# Fails unless the release tag $1 can be released: it exists, it is on the commit that is checked
# out and on origin/main, the files that hold the version and the notes of its component have no
# change that is not committed, it names the version that its component has in the tree, and that
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
    need_committed "$component"
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
# of the other components whose version changed. A version that cannot be read at the commit is
# taken for one that was never released.
pending() {
    local commit="$1" component old new prefix why result=""
    git -C "$ROOT" rev-parse -q --verify "$commit^{commit}" >/dev/null ||
        die "no commit $commit in this checkout"
    for component in "${COMPONENTS[@]}"; do
        new="$("$VERSION_SCRIPT" current "$component")" || exit 1
        prefix="$(component_tag_prefix "$component")"
        if old="$("$VERSION_SCRIPT" current "$component" --at "$commit" 2>&1)"; then
            [[ "$new" != "$old" ]] || continue
            why="the version changed from $old, which was never released, to $new"
        else
            why="its earlier version could not be read ($old)"
            old=""
        fi
        if has_tag "$prefix$new"; then
            echo "$prefix$new already exists; nothing to release" >&2
        elif [[ -n "$old" ]] && has_tag "$prefix$old"; then
            result="${result:+$result$'\n'}$component"
        else
            echo "$component: $why, so this is not a version bump after a release; the tag" \
                "workflow has to be run by hand for $component now, before anything else is" \
                "merged, because components released together are tagged on one commit" >&2
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

# Gives the next version of the line to the components of the group $1 and drafts their notes, as
# the header describes. $2 and $3 are what `sdk-version.sh next` takes. Prints the new version.
bump() {
    local group="$1" kind="$2" flag="${3:-}" names next component file release listed="" i
    local -a components=() files=() releases=()
    case "$kind" in
        major | minor | patch) [[ -z "$flag" || "$flag" == --beta ]] || usage ;;
        beta | release) ;;
        *) usage ;;
    esac
    names="$("$VERSION_SCRIPT" components "$group")" || exit 1
    while IFS= read -r component; do
        components+=("$component")
        listed="${listed:+$listed, }$component"
    done <<<"$names"
    next="$("$VERSION_SCRIPT" next "$kind" ${flag:+"$flag"})" || exit 1
    echo "Bumping $listed to $next." >&2
    check_version "$next" >&2
    # Everything that can refuse the bump comes before the first write.
    for component in "${components[@]}"; do
        release="$(latest "$component")" || exit 1
        releases+=("$release")
        file="$(component_changelog "$component")" || exit 1
        files+=("$file")
        [[ -f "$ROOT/$file" ]] || die "no $ROOT/$file"
        if "$CHANGELOG_SCRIPT" section "$component" "$next" >/dev/null 2>&1; then
            die "$file already has a \`## $next\` section"
        fi
    done
    "$VERSION_SCRIPT" set "$next" "${components[@]}" >&2
    for i in "${!components[@]}"; do
        "$CHANGELOG_SCRIPT" draft "${components[i]}" "$next" ${releases[i]:+"${releases[i]}"} >&2
        if [[ -n "${releases[i]}" ]]; then
            echo "${components[i]}: set to $next; ${files[i]} has a draft of the commits" \
                "since ${releases[i]}" >&2
        else
            echo "${components[i]}: set to $next; ${files[i]} has a draft for the first release," \
                "as nothing is released yet" >&2
        fi
    done
    echo "$next"
}

# Creates an annotated tag at HEAD for the version that each of the components $@ has in the tree,
# if the files that hold their versions and their notes are committed and every one of them passes
# `check`, and prints the tags. See the header.
tag_releases() {
    local component version body seen=" " i
    local -a components=() tags=() titles=() bodies=() created=()
    for component in "$@"; do
        is_component "$component" || exit 1
        [[ "$seen" != *" $component "* ]] || continue
        seen="$seen$component "
        components+=("$component")
    done
    # The checks read the working tree, so what they read must be what the commit holds before
    # the first of them runs.
    for component in "${components[@]}"; do
        need_committed "$component"
    done
    for component in "${components[@]}"; do
        check "$component" >&2
    done
    # The tags and their messages are worked out before the first one is created.
    for component in "${components[@]}"; do
        version="$("$VERSION_SCRIPT" current "$component")" || exit 1
        body="$("$CHANGELOG_SCRIPT" section "$component" "$version")" || exit 1
        tags+=("$(component_tag_prefix "$component")$version")
        titles+=("$(component_title "$component") $version")
        bodies+=("$body")
    done
    for i in "${!tags[@]}"; do
        # The notes are kept as they are: by default git drops every line that starts with `#`
        # from a tag message, such as the headings of the notes.
        if ! git -C "$ROOT" tag -a --cleanup=whitespace "${tags[i]}" -m "${titles[i]}" \
            -m "${bodies[i]}"; then
            if [[ ${#created[@]} -gt 0 ]]; then
                git -C "$ROOT" tag -d "${created[@]}" >&2
            fi
            die "could not create ${tags[i]}; no tag was left behind"
        fi
        created+=("${tags[i]}")
    done
    printf '%s\n' "${tags[@]}"
}

case "${1:-}" in
    latest)
        [[ $# -eq 2 ]] || usage
        latest "$2"
        ;;
    published)
        [[ $# -eq 2 ]] || usage
        published "$2"
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
    bump)
        [[ $# -eq 3 || $# -eq 4 ]] || usage
        bump "${@:2}"
        ;;
    tag)
        [[ $# -ge 2 ]] || usage
        shift
        tag_releases "$@"
        ;;
    *) usage ;;
esac
