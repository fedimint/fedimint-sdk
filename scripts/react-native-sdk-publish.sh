#!/usr/bin/env bash
#
# Publishes the tarballs of `@fedimint/react-native-bindings` and `@fedimint/react-native` to
# npm.
#
#   scripts/react-native-sdk-publish.sh <dir>
#   scripts/react-native-sdk-publish.sh --dry-run <dir>
#
# <dir> holds the two tarballs scripts/react-native-sdk-pack.sh wrote: exactly one
# `fedimint-react-native-bindings-*.tgz` and one `fedimint-react-native-<version>.tgz`, for the
# same version, which has to be a release version (`X.Y.Z-beta.N` or `X.Y.Z`, see
# android-sdk-version.sh). The bindings are published first: the wrapper names their exact
# version as a peer, so it must never be on npm before them. A beta goes to the dist-tag `beta`
# and a release to `latest`, since npm refuses a prerelease without a tag.
#
# The two tarballs were tested as a pair, and only that pair is ever released. A release that
# stopped half way can be run again with the same tarballs: a package whose version is already
# on npm is skipped, and so is one that turns out to be on npm although `npm publish` reported
# an error, as long as npm serves that very tarball. A version that is on npm as any other
# tarball stops the script. That is what building the packages again for a release that already
# published one of them leads to, unless the build comes out byte for byte the same, and the way
# out is the next version.
#
# A dist-tag never moves back either. When the dist-tag a package would be published under is
# already on a newer version, a later release has replaced this one, and the script stops.
#
# Both packages are checked before anything is uploaded: one whose version is on npm has to be
# there as the local tarball, and one whose version is not must not move its dist-tag back. A
# release that is refused there has uploaded nothing. When the script succeeds, the files in
# <dir> are exactly what is on npm, and <dir>/SHA256SUMS lists their SHA-256 digests in the
# format `sha256sum -c` reads, for the GitHub Release to carry.
#
# A registry that cannot be asked stops the script before anything further is published: a
# version that is on npm must not look like one that is not. Publishing uses npm's trusted
# publishing (OIDC), which needs npm 11.5.1 or later and no token, and adds the provenance
# statement by itself.
#
# `--dry-run` only runs `npm publish --dry-run` on both tarballs: it asks the registry nothing
# and writes nothing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION_SCRIPT="$ROOT/scripts/android-sdk-version.sh"
# The only two packages this script publishes, in the order it publishes them.
BINDINGS_NAME=@fedimint/react-native-bindings
WRAPPER_NAME=@fedimint/react-native
# The oldest npm that can publish with trusted publishing.
MIN_NPM=(11 5 1)

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Fails unless the tool $1 is on PATH. $2 says what it is needed for.
need() {
    command -v "$1" >/dev/null || die "$1 is not on PATH: it is needed $2"
}

# Prints the one file in the directory $2 that matches the glob $3, and stops the script when
# there is not exactly one. $1 names what is looked for.
find_one() {
    local what="$1" dir="$2" pattern="$3" file names=""
    local -a found=()
    for file in "$dir"/$pattern; do
        if [[ -f "$file" ]]; then
            found+=("$file")
            names="$names ${file##*/}"
        fi
    done
    if ((${#found[@]} == 0)); then
        die "found no $what in $dir; pack both packages with scripts/react-native-sdk-pack.sh"
    fi
    if ((${#found[@]} > 1)); then
        die "found ${#found[@]} ${what}s in $dir:$names; keep exactly one of each"
    fi
    echo "${found[0]}"
}

# Prints the name and the version of the package in the tarball $1, separated by a space.
identity() {
    local out
    out="$(tar -xzOf "$1" package/package.json 2>/dev/null | node -e '
        const m = JSON.parse(require("fs").readFileSync(0, "utf8"));
        if (typeof m.name !== "string" || typeof m.version !== "string") process.exit(1);
        console.log(m.name + " " + m.version);
    ' 2>/dev/null)" || die "$1 is not a package tarball: it has no usable package/package.json"
    echo "$out"
}

# Prints the integrity of the file $1 the way npm does: `sha512-` and the base64 of its digest.
integrity() {
    node -e '
        const crypto = require("crypto");
        const data = require("fs").readFileSync(process.argv[1]);
        console.log("sha512-" + crypto.createHash("sha512").update(data).digest("base64"));
    ' "$1"
}

# Stops the script unless npm is new enough to publish with trusted publishing.
check_npm() {
    local found i
    found="$(npm --version)" ||
        die "cannot run npm --version; install npm ${MIN_NPM[0]}.${MIN_NPM[1]}.${MIN_NPM[2]}"
    if [[ ! "$found" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
        die "cannot read a version from the output of npm --version: $found"
    fi
    for i in 0 1 2; do
        if ((10#${BASH_REMATCH[i + 1]} > MIN_NPM[i])); then
            return
        fi
        if ((10#${BASH_REMATCH[i + 1]} < MIN_NPM[i])); then
            die "npm $found is too old: publishing with trusted publishing needs npm" \
                "${MIN_NPM[0]}.${MIN_NPM[1]}.${MIN_NPM[2]} or later"
        fi
    done
}

# Succeeds when $1@$2 is on npm and fails when it is not. Stops the script when npm cannot say,
# since an unreachable registry must not look like a version that is not there.
on_npm() {
    local spec="$1@$2" out status=0 code
    out="$(npm view "$spec" version --json 2>/dev/null)" || status=$?
    if ((status == 0)); then
        # Some versions of npm print nothing, and succeed, for a version that does not exist.
        [[ -n "$out" ]] || return 1
        printf '%s' "$out" | node -e '
            const found = JSON.parse(require("fs").readFileSync(0, "utf8"));
            process.exit([].concat(found).includes(process.argv[1]) ? 0 : 1);
        ' "$2" 2>/dev/null ||
            die "npm answered $out when asked for $spec, which this script does not take as" \
                "the version being on npm; check that npm works and run the release again"
        return 0
    fi
    code="$(printf '%s' "$out" | node -e '
        const error = JSON.parse(require("fs").readFileSync(0, "utf8")).error;
        if (error && typeof error.code === "string") console.log(error.code);
    ' 2>/dev/null || true)"
    if [[ "$code" == E404 ]]; then
        return 1
    fi
    if [[ -n "$code" ]]; then
        die "cannot ask npm whether $spec exists: error code $code; nothing further is" \
            "published, run the release again when npm answers"
    fi
    die "cannot ask npm whether $spec exists: npm exited with status $status and printed" \
        "${out:-nothing}; nothing further is published, run the release again when npm answers"
}

# Prints the version the dist-tag $2 of the package $1 is on, or nothing when the package has no
# such dist-tag or was never published. Stops when npm cannot say: a dist-tag that cannot be
# read must not look like one that is not there. It prints, so call it as
# `x="$(dist_tag ...)" || exit 1`: its own exit only leaves the command substitution.
dist_tag() {
    local name="$1" tag="$2" out status=0 code
    out="$(npm view "$name" dist-tags --json 2>/dev/null)" || status=$?
    if ((status == 0)); then
        printf '%s' "$out" | node -e '
            const tags = JSON.parse(require("fs").readFileSync(0, "utf8"));
            if (tags === null || typeof tags !== "object" || Array.isArray(tags)) process.exit(1);
            const version = tags[process.argv[1]];
            if (version === undefined) process.exit(0);
            if (typeof version !== "string") process.exit(1);
            console.log(version);
        ' "$tag" 2>/dev/null ||
            die "npm answered ${out:-nothing} when asked for the dist-tags of $name, which" \
                "this script does not read; check that npm works and run the release again"
        return 0
    fi
    code="$(printf '%s' "$out" | node -e '
        const error = JSON.parse(require("fs").readFileSync(0, "utf8")).error;
        if (error && typeof error.code === "string") console.log(error.code);
    ' 2>/dev/null || true)"
    # A package that was never published has no dist-tag to move.
    [[ "$code" != E404 ]] || return 0
    if [[ -n "$code" ]]; then
        die "cannot read the dist-tags of $name from npm: error code $code; nothing was" \
            "uploaded, run the release again when npm answers"
    fi
    die "cannot read the dist-tags of $name from npm: npm exited with status $status and" \
        "printed ${out:-nothing}; nothing was uploaded, run the release again when npm answers"
}

# Stops the script when the dist-tag $3 of the package $1 is on a version that $2 is not newer
# than: publishing $2 under it would move the dist-tag back.
need_newer_than_tag() {
    local name="$1" version="$2" tag="$3" current
    current="$(dist_tag "$name" "$tag")" || exit 1
    [[ -n "$current" ]] || return 0
    "$VERSION_SCRIPT" check-newer --published "$version" <<<"$current" >/dev/null 2>&1 ||
        die "the dist-tag $tag of $name is on $current, and $version is not newer: publishing" \
            "it would move $tag back. A later release has replaced this one, so nothing was" \
            "uploaded."
}

# Stops the script unless the tarball $1 is the one npm serves for $2@$3, which is on npm.
need_same_tarball() {
    local tarball="$1" spec="$2@$3" want have
    want="$(npm view "$spec" dist.integrity)" ||
        die "cannot read dist.integrity of $spec from npm; run the release again"
    [[ "$want" == sha512-* ]] ||
        die "npm reports the dist.integrity of $spec as ${want:-nothing}, not a sha512 digest"
    have="$(integrity "$tarball")"
    [[ "$have" == "$want" ]] ||
        die "$spec is on npm as another tarball than ${tarball##*/}: npm reports the integrity" \
            "$want and this file has $have. The two packages were tested as the pair in this" \
            "directory, and only that pair is released, so the release stops here. Release" \
            "the next version."
}

# Looks at $2@$3, the package in the tarball $1, before anything is uploaded. Stops the script
# when the version is on npm as another tarball, or when it is not on npm and publishing it
# under the dist-tag $4 would move that dist-tag back.
preflight() {
    local tarball="$1" name="$2" version="$3" tag="$4"
    if on_npm "$name" "$version"; then
        need_same_tarball "$tarball" "$name" "$version"
    else
        need_newer_than_tag "$name" "$version" "$tag"
    fi
}

# Publishes the tarball $1 of $2@$3 under the dist-tag $4, unless that version is on npm.
publish_one() {
    local tarball="$1" name="$2" version="$3" tag="$4"
    if on_npm "$name" "$version"; then
        need_same_tarball "$tarball" "$name" "$version"
        echo "$name@$version is already on npm, skipped"
        return
    fi
    if ! npm publish "$tarball" --tag "$tag" --access public; then
        # The registry can take an upload and still answer with an error, and someone else can
        # have published the version in the meantime. Only the first leaves this tarball on npm.
        on_npm "$name" "$version" ||
            die "npm publish of $name@$version failed and the version is not on npm; nothing" \
                "further was published. Fix the cause and run the release again with the same" \
                "tarballs."
        need_same_tarball "$tarball" "$name" "$version"
        echo "$name@$version is on npm although npm publish reported a failure"
        return
    fi
    echo "published $name@$version under $tag"
}

# Writes SHA256SUMS in the directory $1 for the files $2 and $3 in it.
write_sums() {
    local dir="$1"
    shift
    if command -v sha256sum >/dev/null; then
        (cd "$dir" && sha256sum "$@") >"$dir/SHA256SUMS"
    else
        (cd "$dir" && shasum -a 256 "$@") >"$dir/SHA256SUMS"
    fi
    echo "wrote $dir/SHA256SUMS"
}

dry_run=0
case $# in
    1)
        [[ "$1" != -* ]] || usage
        dir="$1"
        ;;
    2)
        [[ "$1" == --dry-run ]] || usage
        dry_run=1
        dir="$2"
        ;;
    *) usage ;;
esac

need node "to read the tarballs"
need tar "to read the tarballs"
need npm "to publish"
[[ -d "$dir" ]] || die "$dir is not a directory; pass the directory the tarballs are in"
dir="$(cd "$dir" && pwd)"

bindings_tarball="$(find_one "bindings tarball" "$dir" "fedimint-react-native-bindings-*.tgz")"
wrapper_tarball="$(find_one "wrapper tarball" "$dir" "fedimint-react-native-[0-9]*.tgz")"
bindings_identity="$(identity "$bindings_tarball")"
wrapper_identity="$(identity "$wrapper_tarball")"
read -r bindings_name bindings_version <<<"$bindings_identity"
read -r wrapper_name wrapper_version <<<"$wrapper_identity"
[[ "$bindings_name" == "$BINDINGS_NAME" ]] ||
    die "${bindings_tarball##*/} holds $bindings_name, not $BINDINGS_NAME; pack both again"
[[ "$wrapper_name" == "$WRAPPER_NAME" ]] ||
    die "${wrapper_tarball##*/} holds $wrapper_name, not $WRAPPER_NAME; pack both again"
if [[ "$bindings_version" != "$wrapper_version" ]]; then
    die "the tarballs are for different versions: ${bindings_tarball##*/} is $bindings_version" \
        "and ${wrapper_tarball##*/} is $wrapper_version; pack both again"
fi
version="$bindings_version"
# Only a release version has a dist-tag to go to: anything else would land under `latest`.
"$VERSION_SCRIPT" check-newer "$version" </dev/null >/dev/null ||
    die "the tarballs are for $version, which cannot be published; pack a release version"
if [[ "$version" == *-beta.* ]]; then
    tag=beta
else
    tag=latest
fi

if ((dry_run)); then
    npm publish --dry-run "$bindings_tarball" --tag "$tag" --access public ||
        die "npm publish --dry-run of $bindings_name@$version failed"
    npm publish --dry-run "$wrapper_tarball" --tag "$tag" --access public ||
        die "npm publish --dry-run of $wrapper_name@$version failed"
    exit 0
fi

check_npm
# Nothing is uploaded before both packages have passed.
preflight "$bindings_tarball" "$bindings_name" "$version" "$tag"
preflight "$wrapper_tarball" "$wrapper_name" "$version" "$tag"
publish_one "$bindings_tarball" "$bindings_name" "$version" "$tag"
publish_one "$wrapper_tarball" "$wrapper_name" "$version" "$tag"
write_sums "$dir" "${bindings_tarball##*/}" "${wrapper_tarball##*/}"
