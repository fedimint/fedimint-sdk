#!/usr/bin/env bash
#
# Reads and writes the versions of the SDK's components, and holds the version
# grammar and ordering every SDK release in this repository shares.
#
#   scripts/sdk-version.sh components [all|bindings|<component>]
#   scripts/sdk-version.sh current [<component>] [--at <commit>]
#   scripts/sdk-version.sh next <major|minor|patch> [--beta]
#   scripts/sdk-version.sh next <beta|release>
#   scripts/sdk-version.sh set <version> <component>...
#   scripts/sdk-version.sh check-newer [--published] <version> [--except <version>] <released
#   scripts/sdk-version.sh latest <released
#
# A version is `X.Y.Z-beta.N` (a beta) or `X.Y.Z` (a release), and nothing
# else. From a release, `major`, `minor` or `patch` starts the next version,
# at `-beta.1` with --beta. From a beta, `beta` moves to the next beta and
# `release` releases it. A beta is always finished or continued; there is no
# jumping from one to another version.
#
# A release is made of components, which scripts/sdk-components.sh defines: the
# Rust crate, the Android SDK and the React Native packages. Each keeps its
# version in the tree:
#
#   rust          `version` in the [package] table of
#                 rust/fedimint-sdk/Cargo.toml. `set` also writes it to the
#                 `fedimint-sdk` entry of rust/fedimint-sdk/Cargo.lock and to
#                 `fedimintSdkVersion` in ios/Sources/FedimintSdk/Version.swift.
#   android       `fedimintSdk` in android/gradle/libs.versions.toml.
#   react-native  `"version"` in js/react-native/react-native-bindings/package.json
#                 and in js/react-native/react-native/package.json, which must
#                 agree.
#
# The components share one line of versions: a release takes the next version
# of the line and gives it to the components it publishes, all of them or only
# some. So a component can be behind the line, and the version of the line is
# the newest version any component has.
#
# `components` prints the names of all the components, of the `bindings` (the
# ones generated from the crate's UniFFI surface) or of the one named, one per
# line. `current` prints the version of the component named, or of the line
# when none is named; with --at it reads the files of that commit rather than
# the working tree. `next` starts from the version of the line. `set` first
# reads the version in each file of the components named, and fails naming the
# file, before it writes any, if one is missing, holds no version or holds one
# outside the grammar. Then it writes <version> for each component named, and
# reads each back.
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
# The release, bump and tag workflows all go through this script, so they
# cannot disagree about what a valid version is.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/sdk-components.sh
. "$ROOT/scripts/sdk-components.sh"
# The files that hold the versions, relative to the repository.
CATALOG=android/gradle/libs.versions.toml
CRATE_MANIFEST=rust/fedimint-sdk/Cargo.toml
CRATE_LOCK=rust/fedimint-sdk/Cargo.lock
SWIFT_VERSION=ios/Sources/FedimintSdk/Version.swift
BINDINGS_MANIFEST=js/react-native/react-native-bindings/package.json
WRAPPER_MANIFEST=js/react-native/react-native/package.json
# The commit `current --at` reads the files from, as the caller named it and as
# a full hash. Both are empty when the files are read from the working tree.
AT_NAME=""
AT=""
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

# Prints " at <commit>" when the files are read from a commit, for the messages
# that name a file.
at_suffix() {
    [[ -z "$AT" ]] || printf ' at %s' "$AT_NAME"
}

# Prints the file $1, relative to the repository, as it is in the working tree
# or, with `current --at`, in that commit. Fails when there is no such file.
read_file() {
    local path="$1"
    if [[ -z "$AT" ]]; then
        [[ -f "$ROOT/$path" ]] || die "no $path"
        cat "$ROOT/$path"
    else
        git -C "$ROOT" cat-file -e "$AT:$path" 2>/dev/null || die "no $path$(at_suffix)"
        git -C "$ROOT" show "$AT:$path"
    fi
}

# What holds the version in a file of the kind $1, as the messages call it. The
# kinds are `catalog` (the Android version catalog), `manifest` (a Cargo
# manifest), `lock` (a Cargo lockfile), `swift` (the Swift file with the
# crate's version) and `json` (an npm package manifest).
version_name() {
    case "$1" in
        catalog) echo "fedimintSdk version" ;;
        manifest) echo "[package] version" ;;
        lock) echo "fedimint-sdk package version" ;;
        swift) echo "fedimintSdkVersion" ;;
        json) echo '"version"' ;;
    esac
}

# Prints where the version is in a file of the kind $1, as three lines: the sed
# address of the lines that can hold it (empty for the whole file), the text
# before it on its line and the text after it. Each kind of file has other
# `version`s that are not the one: a Cargo manifest has them in its
# dependencies, a lockfile in its other packages and a package manifest in its
# nested objects, which are indented further. The lines of a manifest end at
# the next table and those of a lockfile at the blank line that ends the
# crate's entry, or at the end of the file, so that a version missing from them
# is not found in what follows. The text goes into a regular expression as it
# is, so it must not hold a character that one reads in its own way, such as a
# dot or a bracket.
version_line() {
    case "$1" in
        catalog) printf '%s\n' '' 'fedimintSdk = "' '"' ;;
        manifest) printf '%s\n' '/^\[package\]$/,/^\[/' 'version = "' '"' ;;
        lock) printf '%s\n' '/^name = "fedimint-sdk"$/,/^$/' 'version = "' '"' ;;
        swift) printf '%s\n' '' 'public let fedimintSdkVersion = "' '"' ;;
        json) printf '%s\n' '' '  "version": "' '",' ;;
    esac
}

# Prints the version in the file of the kind $1 that is read from stdin, or
# nothing when it holds none.
extract_version() {
    local address before after
    {
        IFS= read -r address
        IFS= read -r before
        IFS= read -r after
    } < <(version_line "$1")
    sed -n "${address}s/^${before}\(.*\)${after}\$/\1/p"
}

# Writes the version $2 into the file $3, relative to the repository, which is
# of the kind $1. It changes the line extract_version reads and no other.
inject_version() {
    local kind="$1" version="$2" file="$ROOT/$3" address before after
    {
        IFS= read -r address
        IFS= read -r before
        IFS= read -r after
    } < <(version_line "$kind")
    # `-i.bak` then removing the backup is the form both GNU and BSD sed take.
    sed -i.bak "${address}s/^${before}.*${after}\$/${before}${version}${after}/" "$file"
    rm -f "$file.bak"
}

# Prints the version in the file $2, relative to the repository, which is of the
# kind $1. Fails when the file is missing, holds no version or holds one that
# is not in the grammar.
file_version() {
    local kind="$1" path="$2" what version
    what="$(version_name "$kind")"
    version="$(read_file "$path" | extract_version "$kind")" || exit 1
    [[ -n "$version" ]] || die "no $what in $path$(at_suffix)"
    parse "$version" >/dev/null ||
        die "$what is $version in $path$(at_suffix); a version must be X.Y.Z-beta.N or X.Y.Z"
    echo "$version"
}

# Prints "<kind> <file>" for each file that holds the version of the component
# $1, which `set` writes.
version_files() {
    case "$1" in
        rust) printf '%s\n' "manifest $CRATE_MANIFEST" "lock $CRATE_LOCK" "swift $SWIFT_VERSION" ;;
        android) printf '%s\n' "catalog $CATALOG" ;;
        react-native) printf '%s\n' "json $BINDINGS_MANIFEST" "json $WRAPPER_MANIFEST" ;;
        *) die "no version files for $1" ;;
    esac
}

# Prints the version of the component $1. For the Rust crate that is the one in
# its manifest; the lockfile and the Swift file only follow it.
component_version() {
    local bindings wrapper
    case "$1" in
        rust) file_version manifest "$CRATE_MANIFEST" ;;
        android) file_version catalog "$CATALOG" ;;
        react-native)
            bindings="$(file_version json "$BINDINGS_MANIFEST")" || exit 1
            wrapper="$(file_version json "$WRAPPER_MANIFEST")" || exit 1
            [[ "$bindings" == "$wrapper" ]] ||
                die "the React Native packages disagree: $BINDINGS_MANIFEST is $bindings" \
                    "but $WRAPPER_MANIFEST is $wrapper$(at_suffix)"
            echo "$bindings"
            ;;
        *) die "no version files for $1" ;;
    esac
}

# Prints the version of the line: the newest version any component has.
line_version() {
    local component version newest=""
    for component in "${COMPONENTS[@]}"; do
        version="$(component_version "$component")" || exit 1
        if [[ -z "$newest" || "$(compare "$version" "$newest")" == 1 ]]; then
            newest="$version"
        fi
    done
    echo "$newest"
}

components() {
    local group=all
    [[ $# -le 1 ]] || usage
    [[ $# -eq 0 ]] || group="$1"
    case "$group" in
        all) printf '%s\n' "${COMPONENTS[@]}" ;;
        bindings) printf '%s\n' "${BINDINGS[@]}" ;;
        *)
            is_component "$group" || exit 1
            echo "$group"
            ;;
    esac
}

# current [<component>] [--at <commit>], in any order.
current() {
    local component="" have_component=0 have_at=0
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --at)
                [[ $# -ge 2 && -n "$2" && "$2" != -* ]] || usage
                ((!have_at)) || usage
                have_at=1
                AT_NAME="$2"
                shift 2
                ;;
            -*) usage ;;
            *)
                ((!have_component)) || usage
                have_component=1
                component="$1"
                shift
                ;;
        esac
    done
    if ((have_component)); then
        is_component "$component" || exit 1
    fi
    if ((have_at)); then
        AT="$(git -C "$ROOT" rev-parse -q --verify "$AT_NAME^{commit}")" ||
            die "no commit $AT_NAME in this checkout"
    fi
    if ((have_component)); then
        component_version "$component"
    else
        line_version
    fi
}

next() {
    local kind="${1:-}" flag="${2:-}" version major minor patch final beta
    [[ $# -le 2 ]] || usage
    version="$(line_version)"
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

# set <version> <component>...: before the first file is written, checks the
# version, the component names and, for every file of the components named,
# that it exists, has the line that holds the version and that the version
# there is in the grammar. After the last file is written, reads every file
# back.
set_versions() {
    local version="${1:-}" component kind path
    [[ $# -ge 2 ]] || usage
    shift
    parse "$version" >/dev/null || die "$version is not a version: it must be X.Y.Z-beta.N or X.Y.Z"
    for component in "$@"; do
        is_component "$component" || exit 1
    done
    for component in "$@"; do
        while read -r kind path; do
            file_version "$kind" "$path" >/dev/null
        done < <(version_files "$component")
    done
    for component in "$@"; do
        while read -r kind path; do
            inject_version "$kind" "$version" "$path"
        done < <(version_files "$component")
    done
    for component in "$@"; do
        while read -r kind path; do
            [[ "$(file_version "$kind" "$path")" == "$version" ]] ||
                die "could not write $version to $path"
        done < <(version_files "$component")
    done
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
    components)
        shift
        components "$@"
        ;;
    current)
        shift
        current "$@"
        ;;
    next)
        shift
        next "$@"
        ;;
    set)
        shift
        set_versions "$@"
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
