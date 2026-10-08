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
# A release that stopped half way can simply be run again. A package whose version is already on
# npm is skipped, and so is one that turns out to be on npm although `npm publish` reported an
# error. When the local tarball of such a package is not the one npm serves, it is replaced with
# the one npm serves after checking it against the digest npm reports. Whichever run published
# a version, the files in <dir> are then exactly what is on npm, and
# <dir>/SHA256SUMS lists their SHA-256 digests in the format `sha256sum -c` reads, for the
# GitHub Release to carry.
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

DOWNLOAD=""

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Removes a download that was not finished.
cleanup() {
    [[ -z "$DOWNLOAD" ]] || rm -f "$DOWNLOAD"
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

# Replaces the tarball $1 of $2@$3, which is on npm, with the one npm serves when they differ.
reconcile() {
    local tarball="$1" spec="$2@$3" want have url got
    want="$(npm view "$spec" dist.integrity)" ||
        die "cannot read dist.integrity of $spec from npm; run the release again"
    [[ "$want" == sha512-* ]] ||
        die "npm reports the dist.integrity of $spec as ${want:-nothing}, not a sha512 digest"
    have="$(integrity "$tarball")"
    if [[ "$have" == "$want" ]]; then
        return
    fi
    url="$(npm view "$spec" dist.tarball)" ||
        die "cannot read dist.tarball of $spec from npm; run the release again"
    DOWNLOAD="$(mktemp "$(dirname "$tarball")/.download.XXXXXX")"
    curl -fsSL --retry 3 -o "$DOWNLOAD" "$url" || die "cannot download $url"
    got="$(integrity "$DOWNLOAD")"
    [[ "$got" == "$want" ]] ||
        die "the file at $url has the digest $got, not the dist.integrity $want that npm" \
            "reports for $spec; $tarball was kept. Run the release again."
    chmod 644 "$DOWNLOAD"
    mv "$DOWNLOAD" "$tarball"
    DOWNLOAD=""
    echo "replaced ${tarball##*/} with the tarball npm serves for $spec, which differs from it"
}

# Publishes the tarball $1 of $2@$3 under the dist-tag $4, unless that version is on npm.
publish_one() {
    local tarball="$1" name="$2" version="$3" tag="$4"
    if on_npm "$name" "$version"; then
        reconcile "$tarball" "$name" "$version"
        echo "$name@$version is already on npm, skipped"
        return
    fi
    if ! npm publish "$tarball" --tag "$tag" --access public; then
        # The registry can take an upload and still answer with an error, and someone else can
        # have published the version in the meantime. A retry must not fail on publishing over a
        # version that is there, but what is there may not be this tarball.
        on_npm "$name" "$version" ||
            die "npm publish of $name@$version failed and the version is not on npm; nothing" \
                "further was published. Fix the cause and run the release again."
        reconcile "$tarball" "$name" "$version"
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
trap cleanup EXIT

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

need curl "to download a published tarball that differs from the local one"
check_npm
publish_one "$bindings_tarball" "$bindings_name" "$version" "$tag"
publish_one "$wrapper_tarball" "$wrapper_name" "$version" "$tag"
write_sums "$dir" "${bindings_tarball##*/}" "${wrapper_tarball##*/}"
