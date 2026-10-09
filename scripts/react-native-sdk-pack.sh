#!/usr/bin/env bash
#
# Packs `@fedimint/react-native-bindings` and `@fedimint/react-native` into the tarballs a
# release publishes, and refuses tarballs that are not fit to publish.
#
#   scripts/react-native-sdk-pack.sh <version> <out dir>
#   scripts/react-native-sdk-pack.sh verify <version> <dir>
#
# In the tree both packages are private and at version 0.0.0: the version only exists in a
# release tag, and `private` keeps every other way of publishing away from them. A pack writes
# <version> into both manifests, drops `private`, runs `pnpm pack` on both packages into
# <out dir> and checks the result with `verify`. `pnpm pack` is what rewrites the wrapper's
# `workspace:*` range on the bindings to the bindings' exact version, and it reads that version
# from the workspace, which is why the manifests are changed in place. They are put back as
# they were when the script exits, however it exits.
#
# A pack builds nothing. Everything the tarballs consist of must already be on disk, and the
# script lists all that is missing, with the command that produces it. It needs `pnpm`, `node`
# and `tar` on PATH (in this repository: the `.#android` shell) and prints, for each tarball,
# its sizes, file count and SHA-256, and the size of each native library in the bindings.
#
# `verify` checks the two tarballs <dir>/fedimint-react-native-bindings-<version>.tgz and
# <dir>/fedimint-react-native-<version>.tgz and reports every problem it finds, so a release
# can fix them in one go. The rules are the ones whose breaking a user finds out about only
# after the release, or npm only at the very end of it: the manifest and what it points at, the
# files an app needs to build and run the native module, and the two packages naming each other
# at the same version. The release workflow packs with the first form and tests the tarballs it
# gets back; scripts/react-native-sdk-publish.sh publishes them.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION_SCRIPT="$ROOT/scripts/android-sdk-version.sh"
BINDINGS_NAME=@fedimint/react-native-bindings
BINDINGS_DIR=js/react-native/react-native-bindings
WRAPPER_DIR=js/react-native/react-native
XCFRAMEWORK=FedimintReactNativeBindingsFramework.xcframework
# The manifests a pack changes and restores.
MANIFESTS=("$ROOT/$BINDINGS_DIR/package.json" "$ROOT/$WRAPPER_DIR/package.json")

# What must be on disk before a pack, relative to the repository, each group with the command
# that produces it. Without the install there is nothing to run the build with.
INSTALLED=(js/node_modules)
INSTALL_COMMAND="pnpm --dir js install"
# Without these the packages hold no JavaScript, no Expo config plugin and no Android build
# files for the native module.
BUILT=(
    "$BINDINGS_DIR/lib/module/index.js"
    "$BINDINGS_DIR/plugin/build/index.js"
    "$BINDINGS_DIR/android/generated/jni/CMakeLists.txt"
    "$WRAPPER_DIR/lib/module/index.js"
)
BUILD_COMMAND="pnpm --dir js run build:reactnative"
# Without these an Android app builds and then fails when it loads the native module.
ANDROID_LIBRARIES=(
    "$BINDINGS_DIR/android/src/main/jniLibs/arm64-v8a/libfedimint_sdk.so"
    "$BINDINGS_DIR/android/src/main/jniLibs/x86_64/libfedimint_sdk.so"
)
ANDROID_COMMAND="scripts/generate-sdk-rn-bindings.sh"
# Without these an iOS app does not link.
IOS_LIBRARIES=(
    "$BINDINGS_DIR/$XCFRAMEWORK/Info.plist"
    "$BINDINGS_DIR/$XCFRAMEWORK/ios-arm64/libfedimint_sdk.a"
    "$BINDINGS_DIR/$XCFRAMEWORK/ios-arm64_x86_64-simulator/libfedimint_sdk.a"
)
IOS_COMMAND="scripts/assemble-rn-ios-xcframework.sh"

# What every bindings tarball must hold, as tarball entries. Without the native libraries an app
# builds and then fails when it loads the native module.
BINDINGS_LIBRARY_ENTRIES=(
    package/android/src/main/jniLibs/arm64-v8a/libfedimint_sdk.so
    package/android/src/main/jniLibs/x86_64/libfedimint_sdk.so
    "package/$XCFRAMEWORK/Info.plist"
    "package/$XCFRAMEWORK/ios-arm64/libfedimint_sdk.a"
    "package/$XCFRAMEWORK/ios-arm64_x86_64-simulator/libfedimint_sdk.a"
)
# Without these the native module does not compile on Android or iOS, or CocoaPods and the
# React Native CLI do not find it.
BINDINGS_BUILD_ENTRIES=(
    package/ReactNativeBindings.podspec
    package/react-native.config.js
    package/android/build.gradle
    package/android/CMakeLists.txt
    package/android/generated/jni/CMakeLists.txt
    package/ios/ReactNativeBindings.mm
    package/cpp/generated/fedimint_sdk.cpp
)
# Without these the JavaScript API of the native module and the Expo config plugin that sets it
# up in an app are missing.
BINDINGS_SCRIPT_ENTRIES=(
    package/src/generated/fedimint_sdk.ts
    package/plugin/build/index.js
    package/app.plugin.js
)
# Without these an Expo app cannot configure the wrapper, and the package page on npm is empty.
WRAPPER_ENTRIES=(
    package/app.plugin.js
    package/README.md
)
# The name of a library the bindings must not ship. React Native brings its own copy, and a
# second one in the package can clash with it in an app's build.
FORBIDDEN_ENTRY_NAME=libc++_shared.so

SCRATCH=""
MANIFESTS_SAVED=0

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Puts the manifests back as they were, then removes the scratch directory. A manifest that
# cannot be restored leaves the scratch directory with its copy.
cleanup() {
    local i
    if ((MANIFESTS_SAVED)); then
        for i in "${!MANIFESTS[@]}"; do
            cp "$SCRATCH/manifest.$i" "${MANIFESTS[$i]}" || {
                echo "cannot restore ${MANIFESTS[$i]}; its original is $SCRATCH/manifest.$i" >&2
                return 0
            }
        done
    fi
    [[ -z "$SCRATCH" ]] || rm -rf "$SCRATCH"
}

# Fails unless the tool $1 is on PATH. $2 says where to get it.
need() {
    command -v "$1" >/dev/null || die "$1 is not on PATH: $2"
}

# Prints the problems with the tarball $1 of package kind $2 (bindings or wrapper) as $3 and
# $4 (its name and the version it must have), one per line, or nothing when it is fit to
# publish. $5 is a file listing the tarball's entries, one per line. The remaining arguments
# are the entries the tarball must hold.
tarball_problems() {
    local tarball="$1" kind="$2" name="$3" version="$4" entries="$5"
    shift 5
    # The `${...}` in the JavaScript are for node, not for the shell.
    # shellcheck disable=SC2016
    tar -xzOf "$tarball" package/package.json | node -e '
        const fs = require("fs");
        const [kind, name, version, entriesFile, bindingsName, forbidden, ...required] =
            process.argv.slice(1);
        const entries = new Set(fs.readFileSync(entriesFile, "utf8").split("\n").filter(Boolean));
        const text = fs.readFileSync(0, "utf8");
        const quote = (value) => JSON.stringify(value);
        const problems = [];
        let m;
        try {
            m = JSON.parse(text);
        } catch (error) {
            console.log("package/package.json is not valid JSON");
            process.exit(0);
        }

        if (m.name !== name) {
            problems.push(`name is ${quote(m.name)}, expected ${quote(name)}`);
        }
        if (m.version !== version) {
            problems.push(`version is ${quote(m.version)}, expected ${quote(version)}`);
        }
        if ("private" in m) {
            problems.push("has a \"private\" field, which stops npm from publishing it");
        }
        // pnpm leaves a range it does not know in place, in fields such as resolutions.
        const workspaceLines = text.split("\n").filter((line) => line.includes("workspace:"));
        if (workspaceLines.length > 0) {
            const lines = workspaceLines.map((line) => line.trim()).join(" ");
            problems.push(`contains "workspace:" ranges that npm cannot resolve: ${lines}`);
        }
        for (const script of ["preinstall", "install", "postinstall"]) {
            if (m.scripts && Object.prototype.hasOwnProperty.call(m.scripts, script)) {
                problems.push(`scripts.${script} is set, and would run on every install`);
            }
        }
        if (!m.publishConfig || m.publishConfig.access !== "public") {
            problems.push("publishConfig.access is not \"public\"");
        }
        const url = m.repository && m.repository.url;
        const repositoryRe = /^(git\+)?https:\/\/github\.com\/fedimint\/fedimint-sdk(\.git)?$/;
        if (typeof url !== "string") {
            problems.push("repository.url is missing");
        } else if (!repositoryRe.test(url)) {
            problems.push(
                `repository.url is ${quote(url)}, expected` +
                    " https://github.com/fedimint/fedimint-sdk (with git+ before it and .git" +
                    " after it if wanted), which npm checks against the repository the package" +
                    " was built in"
            );
        }

        // Every file the manifest points at must be in the tarball.
        const references = [];
        for (const field of ["main", "module", "types", "source"]) {
            if (typeof m[field] === "string") references.push([field, m[field]]);
        }
        const walk = (label, value) => {
            if (typeof value === "string") {
                if (value.startsWith("./")) references.push([label, value]);
            } else if (value && typeof value === "object") {
                for (const [key, inner] of Object.entries(value)) {
                    walk(`${label}[${quote(key)}]`, inner);
                }
            }
        };
        if ("exports" in m) walk("exports", m.exports);
        for (const [label, reference] of references) {
            if (!entries.has("package/" + reference.replace(/^\.\//, ""))) {
                problems.push(`${label} points at ${reference}, which is not in the tarball`);
            }
        }

        for (const entry of required) {
            if (!entries.has(entry)) problems.push(`missing entry ${entry}`);
        }
        if (kind === "bindings") {
            for (const entry of entries) {
                if (entry.split("/").pop() === forbidden) {
                    problems.push(
                        `contains ${entry}: React Native brings its own copy, and a second one` +
                            " can clash with it, so leave it out of the package"
                    );
                }
            }
        } else {
            const label = `peerDependencies[${quote(bindingsName)}]`;
            const peer = (m.peerDependencies || {})[bindingsName];
            if (peer === undefined) {
                problems.push(`${label} is missing, expected exactly ${quote(version)}`);
            } else if (peer !== version) {
                problems.push(`${label} is ${quote(peer)}, expected exactly ${quote(version)}`);
            }
        }
        problems.forEach((problem) => console.log(problem));
    ' "$kind" "$name" "$version" "$entries" "$BINDINGS_NAME" "$FORBIDDEN_ENTRY_NAME" "$@"
}

# verify <version> <dir>: checks the two tarballs for <version> in <dir>.
verify() {
    local version="$1" dir="$2" kind name file entries count total=0 line
    local -a required
    [[ -d "$dir" ]] || die "$dir is not a directory; pass the directory the tarballs are in"
    dir="$(cd "$dir" && pwd)"
    for kind in bindings wrapper; do
        if [[ "$kind" == bindings ]]; then
            name="$BINDINGS_NAME"
            file="fedimint-react-native-bindings-$version.tgz"
            required=(
                "${BINDINGS_LIBRARY_ENTRIES[@]}"
                "${BINDINGS_BUILD_ENTRIES[@]}"
                "${BINDINGS_SCRIPT_ENTRIES[@]}"
            )
        else
            name=@fedimint/react-native
            file="fedimint-react-native-$version.tgz"
            required=("${WRAPPER_ENTRIES[@]}")
        fi
        if [[ ! -f "$dir/$file" ]]; then
            echo "$file: not found in $dir; pack both packages for $version into it" >&2
            total=$((total + 1))
            continue
        fi
        entries="$SCRATCH/entries"
        if ! tar -tzf "$dir/$file" >"$entries" 2>/dev/null; then
            echo "$file: is not a gzipped tar archive" >&2
            total=$((total + 1))
            continue
        fi
        if ! grep -qxF package/package.json "$entries"; then
            echo "$file: has no package/package.json" >&2
            total=$((total + 1))
            continue
        fi
        tarball_problems "$dir/$file" "$kind" "$name" "$version" "$entries" "${required[@]}" \
            >"$SCRATCH/problems"
        count=0
        while IFS= read -r line; do
            echo "$file: $line" >&2
            count=$((count + 1))
        done <"$SCRATCH/problems"
        if ((count == 0)); then
            echo "$file: fit to publish"
        fi
        total=$((total + count))
    done
    ((total == 0)) || die "found $total problem(s) in the tarballs in $dir; fix them and pack again"
}

# Appends the paths of $2... that are not on disk to MISSING, each with the command $1 that
# produces them.
require_on_disk() {
    local command="$1" path
    shift
    for path in "$@"; do
        if [[ ! -e "$ROOT/$path" ]]; then
            MISSING+=("$path: run $command")
        fi
    done
}

# Prints the report on the tarball $1: its sizes, file count and SHA-256, and the size of each
# native library in it.
report() {
    local tarball="$1" unpacked
    unpacked="$(mktemp -d "$SCRATCH/unpacked.XXXXXX")"
    tar -xzf "$tarball" -C "$unpacked"
    # The `${...}` in the JavaScript are for node, not for the shell.
    # shellcheck disable=SC2016
    node -e '
        const crypto = require("crypto");
        const fs = require("fs");
        const path = require("path");
        const [tarball, root] = process.argv.slice(1);
        const files = [];
        const walk = (dir) => {
            for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
                const full = path.join(dir, entry.name);
                if (entry.isDirectory()) {
                    walk(full);
                } else {
                    const name = path.relative(root, full).split(path.sep).join("/");
                    files.push([name, fs.lstatSync(full).size]);
                }
            }
        };
        walk(root);
        const sizes = (bytes) =>
            `${String(bytes).padStart(12)} bytes  ${(bytes / 1e6).toFixed(1).padStart(8)} MB`;
        const compressed = fs.statSync(tarball).size;
        const unpackedSize = files.reduce((sum, [, size]) => sum + size, 0);
        const digest = crypto.createHash("sha256").update(fs.readFileSync(tarball)).digest("hex");
        console.log(path.basename(tarball));
        console.log(`  compressed ${sizes(compressed)}`);
        console.log(`  unpacked   ${sizes(unpackedSize)}`);
        console.log(`  files      ${String(files.length).padStart(12)}`);
        console.log(`  sha256     ${digest}`);
        const libraries = files
            .filter(([name]) => /libfedimint_sdk\.(so|a)$/.test(name))
            .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
        if (libraries.length > 0) {
            console.log("  native libraries");
            const width = Math.max(...libraries.map(([name]) => name.length));
            for (const [name, size] of libraries) {
                console.log(`    ${name.padEnd(width)} ${sizes(size)}`);
            }
        }
    ' "$tarball" "$unpacked"
    rm -rf "${unpacked:?}"
}

# pack <version> <out dir>
pack() {
    local version="$1" out="$2" i
    MISSING=()
    "$VERSION_SCRIPT" check-newer "$version" </dev/null >/dev/null
    need pnpm "run this in the .#android shell (nix develop --accept-flake-config .#android)"
    require_on_disk "$INSTALL_COMMAND" "${INSTALLED[@]}"
    require_on_disk "$BUILD_COMMAND" "${BUILT[@]}"
    require_on_disk "$ANDROID_COMMAND" "${ANDROID_LIBRARIES[@]}"
    require_on_disk "$IOS_COMMAND" "${IOS_LIBRARIES[@]}"
    if ((${#MISSING[@]} > 0)); then
        {
            echo "cannot pack: what the tarballs are made of is not all on disk, and a pack"
            echo "builds nothing. Missing:"
            printf '  %s\n' "${MISSING[@]}"
        } >&2
        exit 1
    fi

    mkdir -p "$out"
    out="$(cd "$out" && pwd)"
    rm -f "$out"/fedimint-react-native-*.tgz

    # The originals are copied aside first, so that nothing can leave the tree changed.
    for i in "${!MANIFESTS[@]}"; do
        cp "${MANIFESTS[$i]}" "$SCRATCH/manifest.$i"
    done
    MANIFESTS_SAVED=1
    node -e '
        const fs = require("fs");
        const [version, ...files] = process.argv.slice(1);
        for (const file of files) {
            const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
            manifest.version = version;
            delete manifest.private;
            fs.writeFileSync(file, JSON.stringify(manifest, null, 2) + "\n");
        }
    ' "$version" "${MANIFESTS[@]}"

    pnpm --dir "$ROOT/$BINDINGS_DIR" pack --pack-destination "$out"
    pnpm --dir "$ROOT/$WRAPPER_DIR" pack --pack-destination "$out"
    verify "$version" "$out"
    echo
    report "$out/fedimint-react-native-bindings-$version.tgz"
    echo
    report "$out/fedimint-react-native-$version.tgz"
}

if [[ "${1:-}" == verify ]]; then
    [[ $# -eq 3 ]] || usage
    mode=verify
elif [[ $# -eq 2 ]]; then
    mode=pack
else
    usage
fi
need node "it checks the manifests"
need tar "it reads the tarballs"
SCRATCH="$(mktemp -d)"
trap cleanup EXIT
trap 'exit 1' INT TERM HUP
if [[ "$mode" == verify ]]; then
    verify "$2" "$3"
else
    pack "$1" "$2"
fi
