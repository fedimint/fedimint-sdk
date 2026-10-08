#!/usr/bin/env bash
#
# Stages a copy of the bare React Native example as an app of its own, with the two React Native
# packages installed from their tarballs.
#
#   scripts/react-native-sdk-example.sh [--no-install] <dest> <bindings tarball> <wrapper tarball>
#
# Before the packages are published, the release has to prove that the tarballs work, not the
# workspace they were packed from: installed into an app as two plain dependencies, built and run.
# The app is the example in js/examples/react-native. Inside this repository it is wired to the
# workspace in ways that would make such a test pass without touching the tarballs, so the copy
# differs from the example in these ways:
#
# - <dest> is outside the repository, and no directory above it holds a `package.json`, a
#   `pnpm-workspace.yaml` or a `node_modules` directory. Node and npm look up the tree, so any of
#   them would let the app find packages it did not install.
# - Its `package.json` lists `@fedimint/react-native-bindings` and `@fedimint/react-native` as
#   `file:` dependencies on the tarballs, both directly, which is how an app has to list them. The
#   example lists them as `workspace:*`.
# - Every other dependency is pinned to the exact version the workspace lockfile
#   `js/pnpm-lock.yaml` records for the example. The example's ranges (`react` is `>=19.2.4`)
#   mean what the lockfile fixes, while outside the workspace npm would pick the newest match,
#   and React Native only runs with the exact React version its renderer was built for. A
#   dependency the lockfile links to a workspace package is refused: this script cannot install
#   it from a tarball.
# - `ios/Podfile.lock` is removed, since it records paths into the workspace and `pod install`
#   writes a new one.
# - `react-native.config.js` is removed, since it only points React Native's autolinking at the
#   workspace's bindings directory.
# - `metro.config.js` is replaced by React Native's stock one, since the example's maps both
#   packages to their sources in the workspace and watches the workspace root.
# - `index.smoke.js` is added, a copy of scripts/react-native-sdk-smoke.js: an entry file that
#   renders the example's root component and, on top, opens the SDK over an app-private directory
#   and logs one line with the result. The Android build selects it with `ENTRY_FILE`, and
#   scripts/react-native-sdk-smoke.sh waits for that line.
#
# The example's tracked files are copied as they are in the working tree, so build output and
# `node_modules` of a checkout do not come along. Unless --no-install is given, the script then
# runs `npm install` in <dest> and checks that both packages arrived as real directories of the
# tarballs' version and that the bindings still have their native libraries. <dest> must not
# exist or must be an empty directory, and its parent must exist. When the script fails after it
# started writing, <dest> is left in place as evidence.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLE="$ROOT/js/examples/react-native"
LOCKFILE="$ROOT/js/pnpm-lock.yaml"
# The example's entry in the lockfile's list of importers.
IMPORTER=examples/react-native
BINDINGS_NAME=@fedimint/react-native-bindings
WRAPPER_NAME=@fedimint/react-native
# What the installed bindings must still hold, relative to their directory.
NATIVE_FILES=(
    android/src/main/jniLibs/arm64-v8a/libfedimint_sdk.so
    android/src/main/jniLibs/x86_64/libfedimint_sdk.so
    FedimintReactNativeBindingsFramework.xcframework/Info.plist
    FedimintReactNativeBindingsFramework.xcframework/ios-arm64/libfedimint_sdk.a
    FedimintReactNativeBindingsFramework.xcframework/ios-arm64_x86_64-simulator/libfedimint_sdk.a
)

# The app's directory, and whether the script has started to write into it.
DEST=""
WRITING=0

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Says where a failed staging was left.
report_leftover() {
    local status=$?
    if ((status != 0 && WRITING)); then
        echo "the staged copy was left in $DEST for inspection" >&2
    fi
}

# Fails unless the tool $1 is on PATH. $2 says what it is needed for.
need() {
    command -v "$1" >/dev/null || die "$1 is not on PATH: it is needed $2"
}

# Prints the absolute path of the existing file $1, without resolving symbolic links.
absolute_file() {
    echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
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

# Stops the script when the directory $1, or one above it, holds a `package.json`, a
# `pnpm-workspace.yaml` or a `node_modules` directory. $1 exists.
check_surroundings() {
    local dir marker
    dir="$(cd "$1" && pwd -P)"
    while true; do
        for marker in package.json pnpm-workspace.yaml node_modules; do
            if [[ -e "$dir/$marker" ]]; then
                die "$dir holds a $marker: Node and npm look up the tree from the app and would" \
                    "find packages it did not install; stage the app somewhere else"
            fi
        done
        [[ "$dir" != / ]] || break
        dir="$(dirname "$dir")"
    done
}

# Rewrites the `package.json` in the directory $1: the dependencies named $2 and $4 get the
# versions $3 and $5, and every other dependency gets the version the lockfile records for the
# example.
pin_dependencies() {
    node - "$1/package.json" "$LOCKFILE" "$IMPORTER" "$2" "$3" "$4" "$5" <<'NODE'
const fs = require('fs');
const [, , manifestPath, lockPath, importer, ...direct] = process.argv;
const fail = (text) => {
    console.error(text);
    process.exit(1);
};

// The lines of the importer's block: it runs from its `  <importer>:` line to the next line with
// two or fewer leading spaces that is not blank.
const lines = fs.readFileSync(lockPath, 'utf8').split('\n');
const start = lines.indexOf(`  ${importer}:`);
if (start < 0) {
    fail(`${lockPath} has no importer ${importer}; check the lockfile and the example's path`);
}
let end = start + 1;
while (end < lines.length && (lines[end].trim() === '' || /^ {3}/.test(lines[end]))) end++;
const block = lines.slice(start + 1, end);

// A dependency is a line of six spaces and its name, in single quotes when it starts with `@`.
// Its version is the first line of eight spaces starting with `version: ` below it.
const locked = {};
for (let i = 0; i < block.length; i++) {
    const dependency = /^ {6}(?:'([^']+)'|([^\s'][^:]*)):$/.exec(block[i]);
    if (!dependency) continue;
    for (let j = i + 1; j < block.length; j++) {
        if (/^ {0,6}\S/.test(block[j])) break;
        const version = /^ {8}version: (.*)$/.exec(block[j]);
        if (version) {
            locked[dependency[1] || dependency[2]] = version[1];
            break;
        }
    }
}

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const direct_versions = { [direct[0]]: direct[1], [direct[2]]: direct[3] };
for (const section of ['dependencies', 'devDependencies']) {
    for (const name of Object.keys(manifest[section] || {})) {
        if (name in direct_versions) continue;
        const version = locked[name];
        if (version === undefined) {
            fail(`no version for ${name} in the importer ${importer} of ${lockPath}; lock the ` +
                `example's dependencies with pnpm and stage again`);
        }
        if (version.startsWith('link:')) {
            fail(`${name} is linked to the workspace (${version}) and cannot be installed from a ` +
                `tarball; teach scripts/react-native-sdk-example.sh to install it`);
        }
        // pnpm appends the peer set to a version, from the first parenthesis on.
        manifest[section][name] = version.replace(/\(.*$/, '');
    }
}
manifest.dependencies = { ...manifest.dependencies, ...direct_versions };
fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
NODE
}

# Stops the script unless the packages npm installed into the app $1 are the tarballs' version $2
# and the bindings still have their native libraries.
check_install() {
    local app="$1" version="$2" name dir installed file
    for name in "$BINDINGS_NAME" "$WRAPPER_NAME"; do
        dir="$app/node_modules/$name"
        [[ -d "$dir" && ! -L "$dir" ]] ||
            die "$dir is not a real directory: npm did not install $name from its tarball"
        installed="$(node -e '
            console.log(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version);
        ' "$dir/package.json")" || die "cannot read the version of the package in $dir"
        [[ "$installed" == "$version" ]] ||
            die "$dir is version $installed, not the tarball's $version"
    done
    for file in "${NATIVE_FILES[@]}"; do
        [[ -f "$app/node_modules/$BINDINGS_NAME/$file" ]] ||
            die "the installed bindings lack $file: an install step removed it or the tarball" \
                "did not carry it"
    done
}

install=1
if [[ "${1:-}" == --no-install ]]; then
    install=0
    shift
fi
[[ $# -eq 3 && "$1" != -* ]] || usage
dest="${1%/}"
bindings_tarball="$2"
wrapper_tarball="$3"

need git "to list the example's tracked files"
need tar "to copy the example and to read the tarballs"
need node "to read the tarballs and to rewrite the manifest"
((install == 0)) || need npm "to install the tarballs"

for tarball in "$bindings_tarball" "$wrapper_tarball"; do
    [[ -f "$tarball" ]] ||
        die "$tarball does not exist; pack both packages with scripts/react-native-sdk-pack.sh"
done
bindings_tarball="$(absolute_file "$bindings_tarball")"
wrapper_tarball="$(absolute_file "$wrapper_tarball")"
bindings_identity="$(identity "$bindings_tarball")"
wrapper_identity="$(identity "$wrapper_tarball")"
read -r bindings_name bindings_version <<<"$bindings_identity"
read -r wrapper_name wrapper_version <<<"$wrapper_identity"
[[ "$bindings_name" == "$BINDINGS_NAME" ]] ||
    die "$bindings_tarball holds $bindings_name, not $BINDINGS_NAME; pass the bindings tarball" \
        "first and the wrapper tarball second"
[[ "$wrapper_name" == "$WRAPPER_NAME" ]] ||
    die "$wrapper_tarball holds $wrapper_name, not $WRAPPER_NAME; pass the bindings tarball" \
        "first and the wrapper tarball second"
if [[ "$bindings_version" != "$wrapper_version" ]]; then
    die "the tarballs are for different versions: $bindings_tarball is $bindings_version and" \
        "$wrapper_tarball is $wrapper_version; pack both again"
fi

[[ -n "$dest" ]] || die "the destination is /: pass a directory that does not exist yet"
if [[ -e "$dest" ]]; then
    [[ -d "$dest" ]] ||
        die "$dest exists and is not a directory; pass a directory that does not exist yet"
    DEST="$(cd "$dest" && pwd)"
    physical="$(cd "$dest" && pwd -P)"
else
    parent="$(dirname "$dest")"
    [[ -d "$parent" ]] ||
        die "the parent directory of $dest does not exist; create $parent first"
    DEST="$(cd "$parent" && pwd)/$(basename "$dest")"
    physical="$(cd "$parent" && pwd -P)/$(basename "$dest")"
fi
root_physical="$(cd "$ROOT" && pwd -P)"
if [[ "$physical" == "$root_physical" || "$physical" == "$root_physical"/* ]]; then
    die "$DEST is inside this repository ($ROOT); stage the app outside it, where nothing of" \
        "the workspace can be found"
fi
[[ ! -d "$dest" || -z "$(ls -A "$dest")" ]] ||
    die "$dest exists and is not empty; pass a directory that does not exist yet, or an empty one"
check_surroundings "$(dirname "$physical")"

trap report_leftover EXIT
WRITING=1
mkdir -p "$DEST"
(cd "$EXAMPLE" && git ls-files -z | tar --null -T - -cf -) | (cd "$DEST" && tar -xf -)

rm -f "$DEST/ios/Podfile.lock" "$DEST/react-native.config.js"
cat >"$DEST/metro.config.js" <<'METRO'
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

/**
 * Metro configuration
 * https://reactnative.dev/docs/metro
 *
 * @type {import('@react-native/metro-config').MetroConfig}
 */
const config = {};

module.exports = mergeConfig(getDefaultConfig(__dirname), config);
METRO
cp "$ROOT/scripts/react-native-sdk-smoke.js" "$DEST/index.smoke.js"
pin_dependencies "$DEST" "$BINDINGS_NAME" "file:$bindings_tarball" "$WRAPPER_NAME" \
    "file:$wrapper_tarball"

if ((install)); then
    (cd "$DEST" && npm install --no-audit --no-fund)
    check_install "$DEST" "$bindings_version"
fi
echo "staged $DEST"
