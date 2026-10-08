#!/usr/bin/env bash
#
# Reads and bumps the Android SDK's version, and holds the version grammar and
# ordering every SDK release in this repository shares.
#
#   scripts/android-sdk-version.sh current
#   scripts/android-sdk-version.sh next <major|minor|patch> [--beta]
#   scripts/android-sdk-version.sh next <beta|release>
#   scripts/android-sdk-version.sh set <version>
#   scripts/android-sdk-version.sh check-newer [--published] <version> [--except <version>] <released
#   scripts/android-sdk-version.sh latest <released
#
# A version is `X.Y.Z-beta.N` (a beta) or `X.Y.Z` (a release), and nothing
# else. From a release, `major`, `minor` or `patch` starts the next version,
# at `-beta.1` with --beta. From a beta, `beta` moves to the next beta and
# `release` releases it. A beta is always finished or continued; there is no
# jumping from one to another version.
#
# `current`, `next` and `set` read and write the Android SDK's own version,
# `fedimintSdk` in android/gradle/libs.versions.toml. It does not follow
# rust/fedimint-sdk's.
#
# `check-newer` and `latest` only read the already released versions, one per
# line, on stdin. The Android SDK's workflows feed them the `android-sdk-v*`
# tags; the React Native release, scripts/react-native-sdk-release.sh, feeds
# them its tags and the versions on npm. `check-newer` fails unless <version>
# is newer than all of them, so a release can never go out under a version
# that is already taken or lower than the latest. --except leaves one version
# out, which is how a tag's own run is not counted against itself. `latest`
# prints the newest of them, or nothing when nothing is released yet. A line
# that is not a version is ignored, with a note on stderr.
#
# --published is for a list of versions that exist on a package registry.
# Those are all taken, whatever they look like, so a line outside the grammar
# is not ignored: it counts as the release `X.Y.Z` it starts with (so
# `0.0.0-canary-abc123` counts as `0.0.0`), and only a line that does not start
# with a version at all is ignored. A prerelease is lower than its release and
# build metadata does not change precedence, so this can refuse a version
# semver would allow but never accept one it would not. A list of git tags is
# read without it: a tag outside the grammar never released anything, because
# its release run refused it. `latest` never counts such lines, since its output
# is used as a tag name and must be a real released version.
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
# The start of any semver version with a prerelease or build metadata, which
# `check-newer --published` counts as the release it starts with.
RELEASE_PREFIX_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)[-+]'

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
# nothing when there is none. With $2 set to 1 (published versions), a line
# outside the grammar that starts with `X.Y.Z-` or `X.Y.Z+` counts as `X.Y.Z`,
# and the output is "<counted version> <the line it came from>". Of lines that
# count the same, the one that is that version itself is kept, so a released
# `X.Y.Z` is reported as such next to its prereleases.
latest_of() {
    local except="${1:-}" published="${2:-0}" released counted latest="" raw=""
    while IFS= read -r released; do
        if [[ -z "$released" || "$released" == "$except" ]]; then
            continue
        fi
        if parse "$released" >/dev/null; then
            counted="$released"
        elif ((published)) && [[ "$released" =~ $RELEASE_PREFIX_RE ]]; then
            counted="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.${BASH_REMATCH[3]}"
        else
            echo "ignoring $released: not a version" >&2
            continue
        fi
        if [[ -z "$latest" || "$(compare "$counted" "$latest")" == 1 ]] ||
            [[ "$counted" == "$latest" && "$raw" != "$latest" && "$released" == "$counted" ]]; then
            latest="$counted"
            raw="$released"
        fi
    done
    if ((published)) && [[ -n "$latest" ]]; then
        echo "$latest $raw"
    else
        echo "$latest"
    fi
}

check_newer() {
    local published=0 version except="" latest raw shown found
    if [[ "${1:-}" == --published ]]; then
        published=1
        shift
    fi
    version="${1:-}"
    [[ $# -eq 1 || ($# -eq 3 && "$2" == --except) ]] || usage
    [[ $# -eq 3 ]] && except="$3"
    parse "$version" >/dev/null || die "$version is not a version: it must be X.Y.Z-beta.N or X.Y.Z"
    found="$(latest_of "$except" "$published")"
    if [[ -z "$found" ]]; then
        echo "$version: nothing is released yet"
        return
    fi
    # Without --published the line a version came from is the version itself.
    read -r latest raw <<<"$found"
    raw="${raw:-$latest}"
    shown="$raw"
    [[ "$raw" == "$latest" ]] || shown="$raw (counted as $latest)"
    case "$(compare "$version" "$latest")" in
        1) echo "$version is newer than the latest release, $shown" ;;
        0)
            [[ "$raw" != "$version" ]] ||
                die "$version is already released; bump the version first"
            die "$version is not newer than $raw, which is already released and counts as $latest"
            ;;
        *) die "$version is lower than $shown, which is already released" ;;
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
