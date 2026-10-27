#!/usr/bin/env bash
#
# Publishes the tarballs of the npm packages of a component to npm.
#
#   scripts/sdk-npm-publish.sh <component> <dir>
#   scripts/sdk-npm-publish.sh --dry-run <component> <dir>
#
# <component> is one that publishes to npm, `react-native` or `web`, and its packages are the ones
# component_npm_packages names in sdk-components.sh, in the order they are published. <dir> holds
# their tarballs, as scripts/<component>-sdk-pack.sh wrote them: for each package exactly one
# `<name>-<version>.tgz`, where <name> is the package name without its `@` and with `/` replaced
# by `-`, which is what `pnpm pack` names it. Every tarball must hold the package it is named for,
# all of them at the same version, which has to be a release version (`X.Y.Z-beta.N` or `X.Y.Z`,
# see sdk-version.sh). The packages are published in the order of component_npm_packages: the
# React Native wrapper names the exact version of the bindings as a peer, so it must never be on
# npm before them. A beta goes to the dist-tag `beta` and a release to `latest`, since npm refuses
# a prerelease without a tag.
#
# The tarballs of a component were tested together, and only they are ever released. A release
# that stopped half way can be run again with the same tarballs: a package whose version is
# already on npm is skipped, and so is one that turns out to be on npm although `npm publish`
# reported an error, as long as npm serves that very tarball. A version that is on npm as any
# other tarball stops the script. That is what building the packages again for a release that
# already published one of them leads to, unless the build comes out byte for byte the same, and
# the way out is the next version.
#
# A dist-tag never moves back either. When the dist-tag a package would be published under is
# already on a newer version, a later release has replaced this one, and the script stops.
#
# Every package is checked before anything is uploaded: one whose version is on npm has to be
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
# `--dry-run` only runs `npm publish --dry-run` on every tarball: it asks the registry nothing
# and writes nothing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/sdk-components.sh
. "$ROOT/scripts/sdk-components.sh"
VERSION_SCRIPT="$ROOT/scripts/sdk-version.sh"
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
# there is not exactly one. $1 names what is looked for, and $4 is the script that packs it.
find_one() {
    local what="$1" dir="$2" pattern="$3" packer="$4" file names=""
    local -a found=()
    for file in "$dir"/$pattern; do
        if [[ -f "$file" ]]; then
            found+=("$file")
            names="$names ${file##*/}"
        fi
    done
    if ((${#found[@]} == 0)); then
        die "found no $what in $dir; pack the packages with $packer"
    fi
    if ((${#found[@]} > 1)); then
        die "found ${#found[@]} ${what}s in $dir:$names; keep exactly one of each package"
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
            "$want and this file has $have. The tarballs in this directory were tested" \
            "together, and only they are released, so the release stops here. Release the" \
            "next version."
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

# Writes SHA256SUMS in the directory $1 for the files $2... in it.
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

# Prints the name of the tarball of the package $1 without its version, which is what `pnpm pack`
# makes of the name: the `@` dropped and `/` replaced by `-`.
tarball_stem() {
    local stem="${1#@}"
    echo "${stem//\//-}"
}

dry_run=0
case $# in
    2)
        [[ "$1" != -* ]] || usage
        component="$1"
        dir="$2"
        ;;
    3)
        [[ "$1" == --dry-run ]] || usage
        dry_run=1
        component="$2"
        dir="$3"
        ;;
    *) usage ;;
esac

need node "to read the tarballs"
need tar "to read the tarballs"
need npm "to publish"
is_component "$component" || exit 1
packages=()
while IFS= read -r package; do
    packages+=("$package")
done < <(component_npm_packages "$component")
if ((${#packages[@]} == 0)); then
    die "the component $component has no npm package, so there is nothing to publish to npm"
fi
[[ -d "$dir" ]] || die "$dir is not a directory; pass the directory the tarballs are in"
dir="$(cd "$dir" && pwd)"

# The tarball of each package, and the version it holds, in the order of the packages.
tarballs=()
version=""
for package in "${packages[@]}"; do
    tarball="$(find_one "tarball of $package" "$dir" "$(tarball_stem "$package")-[0-9]*.tgz" \
        "scripts/$component-sdk-pack.sh")"
    found="$(identity "$tarball")" || exit 1
    read -r found_name found_version <<<"$found"
    [[ "$found_name" == "$package" ]] ||
        die "${tarball##*/} holds $found_name, not $package; pack the packages again"
    if [[ -n "$version" && "$found_version" != "$version" ]]; then
        die "the tarballs are for different versions: ${tarballs[0]##*/} is $version and" \
            "${tarball##*/} is $found_version; pack the packages again"
    fi
    version="$found_version"
    tarballs+=("$tarball")
done
# Only a release version has a dist-tag to go to: anything else would land under `latest`.
"$VERSION_SCRIPT" check-newer "$version" </dev/null >/dev/null ||
    die "the tarballs are for $version, which cannot be published; pack a release version"
if [[ "$version" == *-beta.* ]]; then
    tag=beta
else
    tag=latest
fi

if ((dry_run)); then
    for i in "${!packages[@]}"; do
        npm publish --dry-run "${tarballs[i]}" --tag "$tag" --access public ||
            die "npm publish --dry-run of ${packages[i]}@$version failed"
    done
    exit 0
fi

check_npm
# Nothing is uploaded before every package has passed.
for i in "${!packages[@]}"; do
    preflight "${tarballs[i]}" "${packages[i]}" "$version" "$tag"
done
for i in "${!packages[@]}"; do
    publish_one "${tarballs[i]}" "${packages[i]}" "$version" "$tag"
done
names=()
for tarball in "${tarballs[@]}"; do
    names+=("${tarball##*/}")
done
write_sums "$dir" "${names[@]}"
