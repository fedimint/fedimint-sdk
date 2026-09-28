#!/usr/bin/env bash
#
# Reads, bumps and checks the Android SDK's version: `fedimintSdk` in
# android/gradle/libs.versions.toml. The version is the Android SDK's own and
# does not follow rust/fedimint-sdk's.
#
#   scripts/android-sdk-version.sh current
#   scripts/android-sdk-version.sh next <major|minor|patch> [--beta]
#   scripts/android-sdk-version.sh next <beta|release>
#   scripts/android-sdk-version.sh set <version>
#   scripts/android-sdk-version.sh check-newer <version> [--except <version>] <released
#   scripts/android-sdk-version.sh latest <released
#
# A version is `X.Y.Z-beta.N` (a beta) or `X.Y.Z` (a release), and nothing
# else. From a release, `major`, `minor` or `patch` starts the next version,
# at `-beta.1` with --beta. From a beta, `beta` moves to the next beta and
# `release` releases it. A beta is always finished or continued; there is no
# jumping from one to another version.
#
# `check-newer` and `latest` read the already released versions, one per
# line, on stdin (the workflows feed them the `android-sdk-v*` tags).
# `check-newer` fails unless <version> is newer than all of them, so a release
# can never go out under a version that is already taken or lower than the
# latest. --except leaves one version out, which is how a tag's own run is not
# counted against itself. `latest` prints the newest of them, or nothing when
# nothing is released yet.
#
# The release, bump and tag workflows (.github/workflows/android-sdk-*.yaml)
# all go through this script, so they cannot disagree about what a valid
# version is.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CATALOG="$ROOT/android/gradle/libs.versions.toml"
# No leading zeros, as in semver. That also keeps bash arithmetic below from
# reading a component as octal.
VERSION_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-beta\.([1-9][0-9]*))?$'

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Prints "major minor patch final beta" for a version: final is 1 for a
# release and 0 for a beta, and beta is the beta's number (0 for a release).
# In that order they sort a beta of X.Y.Z below X.Y.Z itself. Fails on
# anything that is not a version.
parse() {
    [[ "$1" =~ $VERSION_RE ]] || return 1
    if [[ -n "${BASH_REMATCH[4]}" ]]; then
        echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]} ${BASH_REMATCH[3]} 0 ${BASH_REMATCH[5]}"
    else
        echo "${BASH_REMATCH[1]} ${BASH_REMATCH[2]} ${BASH_REMATCH[3]} 1 0"
    fi
}

# Prints -1, 0 or 1 as $1 is lower than, equal to or higher than $2. Both
# must already be valid versions.
compare() {
    local -a a b
    local i
    read -ra a <<<"$(parse "$1")"
    read -ra b <<<"$(parse "$2")"
    for i in 0 1 2 3 4; do
        if ((a[i] < b[i])); then
            echo -1
            return
        fi
        if ((a[i] > b[i])); then
            echo 1
            return
        fi
    done
    echo 0
}

current() {
    local version
    version="$(sed -n 's/^fedimintSdk = "\(.*\)"$/\1/p' "$CATALOG")"
    [[ -n "$version" ]] || die "no fedimintSdk version in $CATALOG"
    parse "$version" >/dev/null ||
        die "fedimintSdk is $version; a version must be X.Y.Z-beta.N or X.Y.Z"
    echo "$version"
}

next() {
    local kind="${1:-}" flag="${2:-}" version major minor patch final beta
    [[ $# -le 2 ]] || usage
    version="$(current)"
    read -r major minor patch final beta <<<"$(parse "$version")"
    case "$kind" in
        major | minor | patch)
            [[ -z "$flag" || "$flag" == --beta ]] || usage
            ((final)) ||
                die "$version is a beta: bump \`beta\` for the next beta, or \`release\` to release it"
            case "$kind" in
                major) major=$((major + 1)) minor=0 patch=0 ;;
                minor) minor=$((minor + 1)) patch=0 ;;
                patch) patch=$((patch + 1)) ;;
            esac
            echo "$major.$minor.$patch${flag:+-beta.1}"
            ;;
        beta | release)
            [[ -z "$flag" ]] || die "--beta only goes with major, minor or patch"
            ((!final)) ||
                die "$version is a release, not a beta: start the next version with major, minor or patch"
            if [[ "$kind" == beta ]]; then
                echo "$major.$minor.$patch-beta.$((beta + 1))"
            else
                echo "$major.$minor.$patch"
            fi
            ;;
        *) usage ;;
    esac
}

set_version() {
    local version="${1:-}"
    [[ $# -eq 1 ]] || usage
    parse "$version" >/dev/null || die "$version is not a version: it must be X.Y.Z-beta.N or X.Y.Z"
    # `-i.bak` then removing the backup is the form both GNU and BSD sed take.
    sed -i.bak "s/^fedimintSdk = \".*\"$/fedimintSdk = \"$version\"/" "$CATALOG"
    rm -f "$CATALOG.bak"
    [[ "$(current)" == "$version" ]] || die "could not write $version to $CATALOG"
}

# Prints the newest version read from stdin, leaving out $1 if given, or
# nothing when there is none.
latest_of() {
    local except="${1:-}" released latest=""
    while IFS= read -r released; do
        if [[ -z "$released" || "$released" == "$except" ]]; then
            continue
        fi
        if ! parse "$released" >/dev/null; then
            echo "ignoring $released: not a Android SDK version" >&2
            continue
        fi
        if [[ -z "$latest" || "$(compare "$released" "$latest")" == 1 ]]; then
            latest="$released"
        fi
    done
    echo "$latest"
}

check_newer() {
    local version="${1:-}" except="" latest
    [[ $# -eq 1 || ($# -eq 3 && "$2" == --except) ]] || usage
    [[ $# -eq 3 ]] && except="$3"
    parse "$version" >/dev/null || die "$version is not a version: it must be X.Y.Z-beta.N or X.Y.Z"
    latest="$(latest_of "$except")"
    if [[ -z "$latest" ]]; then
        echo "$version: nothing is released yet"
        return
    fi
    case "$(compare "$version" "$latest")" in
        1) echo "$version is newer than the latest release, $latest" ;;
        0) die "$version is already released; bump the version first" ;;
        *) die "$version is lower than $latest, which is already released" ;;
    esac
}

case "${1:-}" in
    current)
        [[ $# -eq 1 ]] || usage
        current
        ;;
    next)
        shift
        next "$@"
        ;;
    set)
        shift
        set_version "$@"
        ;;
    check-newer)
        shift
        check_newer "$@"
        ;;
    latest)
        [[ $# -eq 1 ]] || usage
        latest_of
        ;;
    *) usage ;;
esac
