#!/usr/bin/env bash
#
# Decides whether a release of the `fedimint-sdk` crate still has to publish it to crates.io.
#
#   scripts/rust-sdk-publish.sh decide <version> <commit>
#
# <version> is the version the crate has in the tree (`X.Y.Z-beta.N` or `X.Y.Z`, see
# sdk-version.sh) and <commit> the commit the release is made from, as a full hash: 40 lower-case
# hexadecimal digits. Anything else is a mistake in the call, and fails with the usage and status 2.
#
# A release that stopped half way is run again, and crates.io takes a version once, so the second
# run must not publish again: `cargo publish` fails for a version that is there. Nor may a version
# being there pass for this release having published it. A version can never be replaced, so a
# crate that another commit published stands in the way of this release, and taking it for the
# release's own would end the release with code on crates.io that was not the code tested. Cargo
# records the commit it packages a crate from in the crate itself, as `git.sha1` in its
# `.cargo_vcs_info.json`, and that is what is compared with <commit>. A crate from the right commit
# can still lack that commit's code: with `--allow-dirty`, cargo packages a tree that has changes
# that were not committed, and records that as `"dirty": true` in `git`, leaving it out otherwise.
# Such a crate is not what the release tested either, so it is refused like a crate from another
# commit.
#
# `decide` prints one word on stdout, and says what it found on stderr:
#
#   publish   crates.io does not list <version> of the crate: publish it.
#   skip      crates.io lists it, and the file it serves was packaged from <commit>, from a tree
#             without changes that were not committed: there is nothing left to publish. A yanked
#             version is listed too.
#
# It fails, and prints nothing on stdout, when <version> is listed but its file was packaged from
# another commit, or from a modified tree, or does not say from which commit: a version cannot be
# published twice, so the next version has to be released instead. It fails as well when crates.io
# cannot be asked, or when the file cannot be downloaded or read. Such a failure must never look
# like a crate that is not there, or the answer would be to publish.
#
# What crates.io lists is read by `scripts/sdk-release.sh published rust`, the same way the release
# checks read it. The file is downloaded from static.crates.io with curl and read with tar.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE_SCRIPT="$ROOT/scripts/sdk-release.sh"
VERSION_SCRIPT="$ROOT/scripts/sdk-version.sh"
CRATE=fedimint-sdk
# Where crates.io serves the file of each version of the crate, as <name>-<version>.crate.
CRATE_FILE_URL=https://static.crates.io/crates/fedimint-sdk
# crates.io asks every client to say who it is, and sdk-release.sh says it in these words.
USER_AGENT='fedimint-sdk release check (https://github.com/fedimint/fedimint-sdk)'
# The directory that holds the downloaded file, once there is one. It is removed when the script
# ends.
WORK=""
trap '[[ -z "$WORK" ]] || rm -rf "$WORK"' EXIT

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Downloads the file of the version $1 of the crate to the path $2. Stops the script when crates.io
# does not serve it. Like the curl of sdk-release.sh, this one gets /dev/null for its standard
# input, so that it cannot take what the caller still has to read.
download() {
    local version="$1" file="$2" url code status=0
    url="$CRATE_FILE_URL/$CRATE-$version.crate"
    code="$(curl --silent --show-error --max-time 120 --user-agent "$USER_AGENT" \
        --output "$file" --write-out '%{http_code}' "$url" </dev/null)" || status=$?
    ((status == 0)) ||
        die "cannot download $CRATE $version from crates.io: curl exited with status $status;" \
            "check that curl works and that the network is reachable, then retry later"
    [[ "$code" == 200 ]] ||
        die "cannot download $CRATE $version from crates.io: $url answered HTTP $code;" \
            "retry later"
}

# Prints what the text $1 of a `.cargo_vcs_info.json` says about the tree that the crate was
# packaged from: the commit, `git.sha1`, and ` dirty` after it when `git.dirty` is there and is
# anything but false, as the tree then had changes that were not committed. Only the `git` object
# among the members of the record itself is read. Prints nothing when the record is not an object,
# when it has no such `git` object or no commit of 40 lower-case hexadecimal digits in it, or when
# it has any other `sha1`, which could disagree with that one.
read_record() {
    local info keys git found dirty
    # Without its white space, the record that cargo indents reads like the same JSON on one line.
    # No name or value that is read here has any.
    info="$(tr -d '[:space:]' <<<"$1")"
    grep -q '^[{].*[}]$' <<<"$info" || return 0
    keys="$(grep -o '"sha1":' <<<"$info" || true)"
    [[ "$keys" == '"sha1":' ]] || return 0
    # The members of the record itself, without the braces around them. First every object that is
    # the value of a `git` is marked, with < > in place of its braces. Then every other object and
    # array is replaced by ~, the innermost first. That leaves the `git` that is a member of the
    # record marked, and a `git` deeper in the record gone with the object it was in.
    git="$(sed -E -e 's/^\{(.*)\}$/\1/' -e 's/"git":\{([^{}]*)\}/"git":<\1>/g' \
        -e ':nested' -e 's/\{[^{}]*\}|\[[^][]*\]/~/g' -e 't nested' <<<"$info" |
        grep -o '"git":<[^>]*>' || true)"
    [[ -n "$git" && "$git" != *$'\n'* ]] || return 0
    found="$(grep -o '"sha1":"[0-9a-f]\{40\}"' <<<"$git" || true)"
    [[ -n "$found" ]] || return 0
    dirty="$(grep -o '"dirty":[^,>]*' <<<"$git" || true)"
    # The hash is the 40 characters before the closing quote.
    if [[ -z "$dirty" || "$dirty" == '"dirty":false' ]]; then
        echo "${found: -41:40}"
    else
        echo "${found: -41:40} dirty"
    fi
}

# Prints what the record in the file $1 of the version $2 of the crate says about the tree that the
# crate was packaged from, as read_record has it. Prints nothing when the file has no record. Stops
# the script when tar cannot read the file. It prints, so call it as
# `x="$(recorded_tree ...)" || exit 1`: its own exit only leaves the command substitution.
recorded_tree() {
    local file="$1" version="$2" member members info
    member="$CRATE-$version/.cargo_vcs_info.json"
    members="$(tar -tzf "$file")" ||
        die "the file of $CRATE $version from crates.io is not a gzipped tar archive that tar" \
            "can read"
    grep -qxF -- "$member" <<<"$members" || return 0
    info="$(tar -xzOf "$file" "$member")" ||
        die "cannot read $member from the file of $CRATE $version from crates.io"
    read_record "$info"
}

# Prints publish or skip for the version $1 of the crate released from the commit $2, or stops
# the script, as the header describes.
decide() {
    local version="$1" commit="$2" listed file tree packaged
    listed="$("$RELEASE_SCRIPT" published rust)" ||
        die "cannot tell whether crates.io lists $CRATE $version, so nothing is decided and" \
            "nothing is published; run the release again when crates.io answers"
    if ! grep -qxF -- "$version" <<<"$listed"; then
        echo "crates.io does not list $CRATE $version, so it is to be published." >&2
        echo publish
        return
    fi
    WORK="$(mktemp -d)"
    file="$WORK/$CRATE-$version.crate"
    download "$version" "$file"
    tree="$(recorded_tree "$file" "$version")" || exit 1
    packaged="${tree%% *}"
    if [[ -z "$packaged" ]]; then
        die "crates.io lists $CRATE $version, but its file does not say which commit it was" \
            "packaged from, so it cannot be told from a crate that another release published." \
            "This release is of commit $commit. A version cannot be published twice, so release" \
            "the next version instead."
    fi
    if [[ "$packaged" != "$commit" ]]; then
        die "crates.io lists $CRATE $version, packaged from commit $packaged, but this release" \
            "is of commit $commit. A version cannot be published twice, so release the next" \
            "version instead."
    fi
    if [[ "$tree" == *' dirty' ]]; then
        die "crates.io lists $CRATE $version, packaged from a modified tree at commit $commit," \
            "so it does not hold the code of that commit and is not this release's crate. A" \
            "version cannot be published twice, so release the next version instead."
    fi
    echo "crates.io lists $CRATE $version, packaged from commit $commit, the commit of this" \
        "release, so there is nothing to publish." >&2
    echo skip
}

case "${1:-}" in
    decide)
        [[ $# -eq 3 ]] || usage
        "$VERSION_SCRIPT" check-newer "$2" </dev/null >/dev/null || usage
        if [[ ! "$3" =~ ^[0-9a-f]{40}$ ]]; then
            echo "'$3' is not a full commit hash: it must be 40 lower-case hexadecimal digits" >&2
            usage
        fi
        decide "$2" "$3"
        ;;
    *) usage ;;
esac
