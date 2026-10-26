#!/usr/bin/env bash
#
# Packs `@fedimint/web-sdk` into the tarball a release publishes, and refuses tarballs that are
# not fit to publish.
#
#   scripts/web-sdk-pack.sh <version> <out dir>
#   scripts/web-sdk-pack.sh verify <version> <dir>
#
# In the tree the package is private and at version 0.0.0, which keeps every other way of
# publishing away from it. A pack writes <version> into the manifest, drops `private`, runs
# `pnpm pack` into <out dir> and checks the result with `verify`. The manifest is restored to
# its original state when the script exits, however it exits.
#
# A pack builds nothing. Everything the tarball consists of must already be on disk, and the
# script lists all that is missing, with the command that produces it. It needs `pnpm`, `node`
# and `tar` on PATH and prints the tarball's sizes, file count, SHA-256 and the size of the
# WebAssembly module.
#
# `verify` checks the tarball <dir>/fedimint-web-sdk-<version>.tgz and reports every problem
# it finds, so a release can fix them in one go. The rules are the ones whose breaking a user
# finds out about only after the release, or npm only at the very end of it: the manifest and
# what it points at, the files an app needs to load and run the worker, and the absence of
# development files or duplicate wasm binaries.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION_SCRIPT="$ROOT/scripts/sdk-version.sh"
PACKAGE_NAME="@fedimint/web-sdk"
PACKAGE_DIR="js/web/web-sdk"
MANIFEST="$ROOT/$PACKAGE_DIR/package.json"
TARBALL_BASE="fedimint-web-sdk"

# What must be on disk before a pack, relative to the repository, each group with the command
# that produces it.
INSTALLED=(js/node_modules)
INSTALL_COMMAND="pnpm --dir js install"

BUILT=(
    "$PACKAGE_DIR/dist/index.js"
    "$PACKAGE_DIR/dist/index.d.ts"
    "$PACKAGE_DIR/dist/worker.js"
    "$PACKAGE_DIR/dist/worker.d.ts"
    "$PACKAGE_DIR/dist/generated/fedimint_sdk_bg.js"
)
BUILD_COMMAND="pnpm --dir $PACKAGE_DIR run build"

WASM_MODULE=("$PACKAGE_DIR/dist/generated/fedimint_sdk.wasm")
WASM_COMMAND="scripts/nix-build-sdk-wasm.sh (or pnpm --dir $PACKAGE_DIR run generate)"

# What every web-sdk tarball must hold, as tarball entries.
REQUIRED_ENTRIES=(
    package/dist/index.js
    package/dist/index.d.ts
    package/dist/worker.js
    package/dist/worker.d.ts
    package/dist/generated/fedimint_sdk_bg.js
    package/dist/generated/fedimint_sdk.wasm
    package/README.md
)

SCRATCH=""
MANIFEST_SAVED=0

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Puts the manifest back as it was, then removes the scratch directory.
cleanup() {
    if ((MANIFEST_SAVED)); then
        if [[ -f "$SCRATCH/manifest.json" ]]; then
            cp "$SCRATCH/manifest.json" "$MANIFEST" || {
                echo "cannot restore $MANIFEST; its original is $SCRATCH/manifest.json" >&2
                return 0
            }
        fi
    fi
    [[ -z "$SCRATCH" ]] || rm -rf "$SCRATCH"
}

need() {
    command -v "$1" >/dev/null || die "$1 is not on PATH: $2"
}

# Prints the problems with the tarball $1 as $2 and $3 (its name and the version it must have),
# one per line, or nothing when it is fit to publish. $4 is a file listing the tarball entries.
# The remaining arguments are the entries the tarball must hold.
tarball_problems() {
    local tarball="$1" name="$2" version="$3" entries="$4"
    shift 4
    # The `${...}` in the JavaScript are for node, not for the shell.
    # shellcheck disable=SC2016
    tar -xzOf "$tarball" package/package.json 2>/dev/null | node -e '
        const fs = require("fs");
        const [name, version, entriesFile, ...required] = process.argv.slice(1);
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
        const workspaceLines = text.split("\n").filter((line) => line.includes("workspace:"));
        if (workspaceLines.length > 0) {
            const lines = workspaceLines.map((line) => line.trim()).join(" ");
            problems.push(`contains "workspace:" ranges that npm cannot resolve: ${lines}`);
        }
        if (m.scripts && typeof m.scripts === "object") {
            for (const script of ["preinstall", "install", "postinstall"]) {
                if (Object.prototype.hasOwnProperty.call(m.scripts, script)) {
                    problems.push(`scripts.${script} is set, and would run on every install`);
                }
            }
        }
        if (!m.publishConfig || typeof m.publishConfig !== "object" || m.publishConfig.access !== "public") {
            problems.push("publishConfig.access is not \"public\"");
        }
        const url = m.repository && typeof m.repository === "object" && m.repository.url;
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
        for (const field of ["main", "module", "types"]) {
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
        if ("exports" in m && typeof m.exports === "object" && m.exports !== null) {
            walk("exports", m.exports);
            if (!m.exports["./package.json"]) {
                problems.push("exports[\"./package.json\"] is missing, expected \"./package.json\"");
            } else if (m.exports["./package.json"] !== "./package.json") {
                problems.push(`exports["./package.json"] is ${quote(m.exports["./package.json"])}, expected "./package.json"`);
            }
        } else {
            problems.push("exports[\"./package.json\"] is missing, expected \"./package.json\"");
        }
        for (const [label, reference] of references) {
            if (!entries.has("package/" + reference.replace(/^\.\//, ""))) {
                problems.push(`${label} points at ${reference}, which is not in the tarball`);
            }
        }

        for (const entry of required) {
            if (!entries.has(entry)) problems.push(`missing entry ${entry}`);
        }

        // Forbidden entries check: no source ts files or packaging scripts in tarball.
        for (const entry of entries) {
            if (entry.startsWith("package/src/")) {
                problems.push(`contains ${entry}: source files must not be in the published package to prevent duplicate wasm bloat`);
            }
            if (entry.startsWith("package/scripts/")) {
                problems.push(`contains ${entry}: packaging helper scripts must not be in the published package`);
            }
        }

        problems.forEach((problem) => console.log(problem));
    ' "$name" "$version" "$entries" "$@"
}

# Checks that the WASM module inside the tarball is valid WebAssembly and adequately sized.
verify_wasm_module() {
    local tarball="$1" wasm_entry="package/dist/generated/fedimint_sdk.wasm"
    # shellcheck disable=SC2016
    tar -xzOf "$tarball" "$wasm_entry" 2>/dev/null | node -e '
        const fs = require("fs");
        const buf = fs.readFileSync(0);
        if (buf.length === 0) {
            console.log("wasm module is empty (0 bytes)");
            console.log("wasm module is not a valid WebAssembly binary");
        } else {
            if (buf.length < 100000) {
                console.log(`wasm module is only ${buf.length} bytes; expected full production build (> 100 KB)`);
            }
            // WASM binary magic number: \0asm (0x00, 0x61, 0x73, 0x6d)
            if (buf.length < 4 || buf[0] !== 0x00 || buf[1] !== 0x61 || buf[2] !== 0x73 || buf[3] !== 0x6d) {
                console.log("wasm module does not start with valid WebAssembly magic bytes (\\0asm)");
            } else if (buf.length < 8 || buf[4] !== 0x01 || buf[5] !== 0x00 || buf[6] !== 0x00 || buf[7] !== 0x00) {
                console.log("wasm module has an invalid WebAssembly version header (expected version 1)");
            } else if (!WebAssembly.validate(buf)) {
                console.log("wasm module is not a valid WebAssembly binary");
            }
        }
    '
}

# verify <version> <dir>: checks the tarball for <version> in <dir>.
verify() {
    local version="$1" dir="$2" file entries count total=0 line wasm_problems
    [[ -d "$dir" ]] || die "$dir is not a directory; pass the directory the tarball is in"
    dir="$(cd "$dir" && pwd)"
    file="$TARBALL_BASE-$version.tgz"

    if [[ ! -f "$dir/$file" ]]; then
        echo "$file: not found in $dir; pack the package for $version into it" >&2
        return 1
    fi

    entries="$SCRATCH/entries"
    if ! tar -tzf "$dir/$file" >"$entries" 2>/dev/null; then
        echo "$file: is not a gzipped tar archive" >&2
        return 1
    fi

    if ! grep -qxF package/package.json "$entries"; then
        echo "$file: has no package/package.json" >&2
        return 1
    fi

    tarball_problems "$dir/$file" "$PACKAGE_NAME" "$version" "$entries" "${REQUIRED_ENTRIES[@]}" \
        >"$SCRATCH/problems"
    count=0
    while IFS= read -r line; do
        echo "$file: $line" >&2
        count=$((count + 1))
    done <"$SCRATCH/problems"

    wasm_problems="$(verify_wasm_module "$dir/$file")"
    if [[ -n "$wasm_problems" ]]; then
        while IFS= read -r line; do
            echo "$file: $line" >&2
            count=$((count + 1))
        done <<<"$wasm_problems"
    fi

    total=$((total + count))
    if ((count == 0)); then
        echo "$file: fit to publish"
    fi

    ((total == 0)) || die "found $total problem(s) in the tarball in $dir; fix them and pack again"
}

require_on_disk() {
    local command="$1" path
    shift
    for path in "$@"; do
        if [[ ! -e "$ROOT/$path" ]]; then
            MISSING+=("$path: run $command")
        fi
    done
}

# Prints report on the tarball $1: its sizes, file count, SHA-256, and WASM binary size.
report() {
    local tarball="$1" unpacked
    unpacked="$(mktemp -d "$SCRATCH/unpacked.XXXXXX")"
    tar -xzf "$tarball" -C "$unpacked" 2>/dev/null
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
        const wasm = files.find(([name]) => name.endsWith("fedimint_sdk.wasm"));
        if (wasm) {
            console.log("  wasm module");
            console.log(`    ${wasm[0].padEnd(40)} ${sizes(wasm[1])}`);
        }
    ' "$tarball" "$unpacked"
    rm -rf "${unpacked:?}"
}

# pack <version> <out dir>
pack() {
    local version="$1" out="$2"
    MISSING=()
    "$VERSION_SCRIPT" check-newer "$version" </dev/null >/dev/null
    need pnpm "run this in the development shell"
    need node "it checks and updates the manifest"
    need tar "it reads and packs the tarball"

    require_on_disk "$INSTALL_COMMAND" "${INSTALLED[@]}"
    require_on_disk "$BUILD_COMMAND" "${BUILT[@]}"
    require_on_disk "$WASM_COMMAND" "${WASM_MODULE[@]}"
    if ((${#MISSING[@]} > 0)); then
        {
            echo "cannot pack: what the tarball is made of is not all on disk, and a pack"
            echo "builds nothing. Missing:"
            printf '  %s\n' "${MISSING[@]}"
        } >&2
        exit 1
    fi

    mkdir -p "$out"
    out="$(cd "$out" && pwd)"
    rm -f "$out"/$TARBALL_BASE-*.tgz

    # Save original manifest before mutation
    cp "$MANIFEST" "$SCRATCH/manifest.json"
    MANIFEST_SAVED=1

    node -e '
        const fs = require("fs");
        const [version, file] = process.argv.slice(1);
        const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
        manifest.version = version;
        delete manifest.private;
        fs.writeFileSync(file, JSON.stringify(manifest, null, 2) + "\n");
    ' "$version" "$MANIFEST"

    pnpm --dir "$ROOT/$PACKAGE_DIR" pack --pack-destination "$out"

    # Restore manifest immediately after packaging
    if ((MANIFEST_SAVED)); then
        cp "$SCRATCH/manifest.json" "$MANIFEST"
        MANIFEST_SAVED=0
    fi

    verify "$version" "$out"
    echo
    report "$out/$TARBALL_BASE-$version.tgz"
}

if [[ "${1:-}" == verify ]]; then
    [[ $# -eq 3 ]] || usage
    mode=verify
elif [[ $# -eq 2 ]]; then
    mode=pack
else
    usage
fi

need node "it checks the manifest"
need tar "it reads the tarball"
SCRATCH="$(mktemp -d)"
trap cleanup EXIT
trap 'exit 1' INT TERM HUP

if [[ "$mode" == verify ]]; then
    verify "$2" "$3"
else
    pack "$1" "$2"
fi
