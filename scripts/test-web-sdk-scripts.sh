#!/usr/bin/env bash
#
# Tests scripts/web-sdk-pack.sh, which packs @fedimint/web-sdk into a tarball and checks its
# fitness to publish.
#
#   scripts/test-web-sdk-scripts.sh
#
# The pack and verify commands are run against small mock archives and throwaway trees.
# Nothing in this checkout is modified, and nothing beyond bash, git, node and tar is needed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACK="$ROOT/scripts/web-sdk-pack.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

passed=0
failed=0

pass() {
    passed=$((passed + 1))
    echo "  ok    $1"
}

fail() {
    failed=$((failed + 1))
    echo "  FAIL  $1"
    [[ -z "${2:-}" ]] || printf '        %s\n' "${2//$'\n'/$'\n'        }"
}

# expect_ok <name> <expected stdout> -- <command...>
expect_ok() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        if [[ "$out" == "$want" ]]; then
            pass "$name"
        else
            fail "$name" "expected: $want"$'\n'"got: $out"
        fi
    else
        fail "$name" "exited non-zero: $out"
    fi
}

# expect_fail <name> <expected text in output> -- <command...>
expect_fail() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        fail "$name" "expected a failure, got: $out"
    elif [[ "$out" == *"$want"* ]]; then
        pass "$name"
    else
        fail "$name" "expected the error to mention: $want"$'\n'"got: $out"
    fi
}

# expect_usage <name> -- <command...>: fails with status 2 and prints the usage lines.
expect_usage() {
    local name="$1" out status=0
    shift 2
    out="$("$@" 2>&1)" || status=$?
    if [[ "$status" -eq 2 && "$out" == *usage* ]]; then
        pass "$name"
    else
        fail "$name" "expected exit status 2 and a usage message, got status $status: $out"
    fi
}

# expect_equal <name> <expected> <actual>
expect_equal() {
    if [[ "$3" == "$2" ]]; then
        pass "$1"
    else
        fail "$1" "expected: $2"$'\n'"got: $3"
    fi
}

# expect_contains <name> <expected text> <actual>
expect_contains() {
    if [[ "$3" == *"$2"* ]]; then
        pass "$1"
    else
        fail "$1" "expected the text to mention: $2"$'\n'"got: $3"
    fi
}

# run <command...>: runs the command and leaves output in $OUT and status in $STATUS.
run() {
    STATUS=0
    OUT="$("$@" 2>&1)" || STATUS=$?
}

same() {
    if cmp -s "$1" "$2"; then
        echo same
    else
        echo different
    fi
}

sha256() {
    node -e '
        const crypto = require("crypto");
        const data = require("fs").readFileSync(process.argv[1]);
        console.log(crypto.createHash("sha256").update(data).digest("hex"));
    ' "$1"
}

PACKAGE_NAME="@fedimint/web-sdk"
VERSION="0.1.0-beta.1"
TARBALL_BASE="fedimint-web-sdk"
TARBALL="$TARBALL_BASE-$VERSION.tgz"
REPO_URL="https://github.com/fedimint/fedimint-sdk.git"

REQUIRED_FILES=(
    dist/index.js
    dist/index.d.ts
    dist/worker.js
    dist/worker.d.ts
    dist/generated/fedimint_sdk_bg.js
    dist/generated/fedimint_sdk.wasm
    README.md
)

# make_mock_wasm <file> [byte_length] [bad_magic] [bad_version] [corrupt_body]
make_mock_wasm() {
    local file="$1" size="${2:-120000}" bad_magic="${3:-0}" bad_version="${4:-0}" corrupt_body="${5:-0}"
    mkdir -p "$(dirname "$file")"
    node -e '
        const fs = require("fs");
        const [file, sizeStr, badMagicStr, badVersionStr, corruptBodyStr] = process.argv.slice(1);
        const size = parseInt(sizeStr, 10);
        const badMagic = badMagicStr === "1";
        const badVersion = badVersionStr === "1";
        const corruptBody = corruptBodyStr === "1";

        if (size === 0) {
            fs.writeFileSync(file, Buffer.alloc(0));
            process.exit(0);
        }

        const header = Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
        if (badMagic) {
            header[0] = 0x7f; header[1] = 0x45; header[2] = 0x4c; header[3] = 0x46; // ELF
        } else if (badVersion) {
            header[4] = 0x02; header[5] = 0x00; header[6] = 0x00; header[7] = 0x00;
        }

        if (corruptBody || badMagic || badVersion || size <= 13) {
            const buf = Buffer.alloc(size);
            header.copy(buf, 0, 0, Math.min(8, size));
            if (corruptBody && size > 8) {
                buf.fill(0xff, 8);
            }
            fs.writeFileSync(file, buf);
            process.exit(0);
        }

        // Construct a syntactically valid WebAssembly module of exact `size` using a custom section
        const name = Buffer.from("test");
        const nameLen = Buffer.from([name.length]);
        const payloadLen = size - 8 - 1 - 4; // header(8) + section_id(1) + leb128_len(4)
        const dataLen = payloadLen - 1 - name.length;
        const data = Buffer.alloc(Math.max(0, dataLen));
        const payload = Buffer.concat([nameLen, name, data]);
        const len = payload.length;
        const leb128 = Buffer.from([
            (len & 0x7f) | 0x80,
            ((len >> 7) & 0x7f) | 0x80,
            ((len >> 14) & 0x7f) | 0x80,
            (len >> 21) & 0x7f
        ]);
        const wasm = Buffer.concat([header, Buffer.from([0x00]), leb128, payload]);
        fs.writeFileSync(file, wasm);
    ' "$file" "$size" "$bad_magic" "$bad_version" "$corrupt_body"
}

manifest() {
    local ver="$1"
    cat <<EOF
{
  "name": "$PACKAGE_NAME",
  "version": "$ver",
  "type": "module",
  "main": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "exports": {
    ".": {
      "types": "./dist/index.d.ts",
      "default": "./dist/index.js"
    },
    "./worker": {
      "types": "./dist/worker.d.ts",
      "default": "./dist/worker.js"
    },
    "./package.json": "./package.json"
  },
  "repository": {
    "type": "git",
    "url": "$REPO_URL",
    "directory": "js/web/web-sdk"
  },
  "publishConfig": {
    "access": "public"
  }
}
EOF
}

fill() {
    local ver="$1" dir="$2"
    for file in "${REQUIRED_FILES[@]}"; do
        mkdir -p "$dir/$(dirname "$file")"
        if [[ "$file" == "dist/generated/fedimint_sdk.wasm" ]]; then
            make_mock_wasm "$dir/$file" 120000 0
        else
            echo "mock content for $file" >"$dir/$file"
        fi
    done
    manifest "$ver" >"$dir/package.json"
}

make_tarball() {
    local dir="$1" ver="$2" stage
    mkdir -p "$dir"
    stage="$(mktemp -d "$WORK/stage.XXXX")"
    mkdir "$stage/package"
    fill "$ver" "$stage/package"
    tar -czf "$dir/$TARBALL_BASE-$ver.tgz" -C "$stage" package 2>/dev/null
    rm -rf "$stage"
}

valid_dir() {
    local dir
    dir="$(mktemp -d "$WORK/tgz.XXXX")"
    make_tarball "$dir" "$VERSION"
    echo "$dir"
}

rewrite() {
    local tgz="$1" tmp
    shift
    tmp="$(mktemp -d "$WORK/rewrite.XXXX")"
    tar -xzf "$tgz" -C "$tmp" 2>/dev/null
    "$@" "$tmp/package"
    tar -czf "$tgz" -C "$tmp" package 2>/dev/null
    rm -rf "$tmp"
}

drop() {
    rm -f "${2:?}/$1"
}

add() {
    mkdir -p "$(dirname "$2/$1")"
    echo "added content" >"$2/$1"
}

rewrite_wasm() {
    local pkg_dir="${!#}"
    local size="${1:-120000}" bad_magic="${2:-0}" bad_version="${3:-0}" corrupt_body="${4:-0}"
    if [ "$#" -eq 2 ]; then
        bad_magic=0; bad_version=0; corrupt_body=0
    elif [ "$#" -eq 3 ]; then
        bad_version=0; corrupt_body=0
    elif [ "$#" -eq 4 ]; then
        corrupt_body=0
    fi
    make_mock_wasm "$pkg_dir/dist/generated/fedimint_sdk.wasm" "$size" "$bad_magic" "$bad_version" "$corrupt_body"
}

edit_manifest() {
    node -e '
        const fs = require("fs");
        const m = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
        new Function("m", process.argv[1])(m);
        fs.writeFileSync(process.argv[2], JSON.stringify(m, null, 2) + "\n");
    ' "$1" "$2/package.json"
}

break_manifest() {
    local name="$1" want="$2" code="$3" d
    d="$(valid_dir)"
    rewrite "$d/$TARBALL" edit_manifest "$code"
    expect_fail "$name" "$TARBALL: $want" -- "$PACK" verify "$VERSION" "$d"
}

break_entries() {
    local name="$1" want="$2" d
    shift 2
    d="$(valid_dir)"
    rewrite "$d/$TARBALL" "$@"
    expect_fail "$name" "$TARBALL: $want" -- "$PACK" verify "$VERSION" "$d"
}

echo "verify"

d="$(valid_dir)"
expect_ok "a valid tarball is fit to publish" \
    "$TARBALL: fit to publish" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" edit_manifest 'm.repository.url = "https://github.com/fedimint/fedimint-sdk"'
expect_ok "a repository URL without .git is accepted" \
    "$TARBALL: fit to publish" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" edit_manifest 'm.repository.url = "git+https://github.com/fedimint/fedimint-sdk.git"'
expect_ok "a repository URL with git+ prefix is accepted" \
    "$TARBALL: fit to publish" -- \
    "$PACK" verify "$VERSION" "$d"

# Test raw malformed JSON directly
d="$(valid_dir)"
tmp="$(mktemp -d "$WORK/malformed.XXXX")"
tar -xzf "$d/$TARBALL" -C "$tmp"
echo "not valid json {" >"$tmp/package/package.json"
tar -czf "$d/$TARBALL" -C "$tmp" package
rm -rf "$tmp"
expect_fail "malformed json is reported" "$TARBALL: package/package.json is not valid JSON" -- \
    "$PACK" verify "$VERSION" "$d"

break_manifest "private is refused" 'has a "private" field' 'm.private = true'
break_manifest "a different version is refused" \
    'version is "0.2.0", expected "0.1.0-beta.1"' 'm.version = "0.2.0"'
break_manifest "a different name is refused" \
    'name is "@fedimint/other", expected "@fedimint/web-sdk"' 'm.name = "@fedimint/other"'
break_manifest "a workspace range in dependencies is refused" \
    'contains "workspace:" ranges' 'm.dependencies = { "@fedimint/core": "workspace:*" }'
break_manifest "a workspace range in devDependencies is refused" \
    'contains "workspace:" ranges' 'm.devDependencies = { "@fedimint/core": "workspace:^" }'
break_manifest "a preinstall script is refused" 'scripts.preinstall is set' \
    'm.scripts = { preinstall: "node build.js" }'
break_manifest "an install script is refused" 'scripts.install is set' \
    'm.scripts = { install: "node build.js" }'
break_manifest "a postinstall script is refused" 'scripts.postinstall is set' \
    'm.scripts = { postinstall: "node build.js" }'
break_manifest "missing publishConfig is refused" 'publishConfig.access is not "public"' \
    'delete m.publishConfig'
break_manifest "restricted publishConfig is refused" 'publishConfig.access is not "public"' \
    'm.publishConfig.access = "restricted"'
break_manifest "missing repository is refused" 'repository.url is missing' \
    'delete m.repository'
break_manifest "another repository is refused" \
    'repository.url is "https://github.com/someone/else.git", expected' \
    'm.repository.url = "https://github.com/someone/else.git"'
break_manifest "main pointing at missing file is refused" \
    'main points at ./dist/missing.js, which is not in the tarball' \
    'm.main = "./dist/missing.js"'
break_manifest "types pointing at missing file is refused" \
    'types points at ./dist/missing.d.ts, which is not in the tarball' \
    'm.types = "./dist/missing.d.ts"'
break_manifest "exports pointing at missing file is refused" \
    'exports["./worker"]["default"] points at ./dist/missing-worker.js, which is not in the tarball' \
    'm.exports["./worker"].default = "./dist/missing-worker.js"'
break_manifest "missing ./package.json in exports is refused" \
    'exports["./package.json"] is missing, expected "./package.json"' \
    'delete m.exports["./package.json"]'
break_manifest "invalid ./package.json export target is refused" \
    'exports["./package.json"] is "./dist/package.json", expected "./package.json"' \
    'm.exports["./package.json"] = "./dist/package.json"'
break_manifest "manifest with missing exports field is refused" \
    'exports["./package.json"] is missing, expected "./package.json"' \
    'delete m.exports'
break_manifest "null exports field in manifest is refused" \
    'exports["./package.json"] is missing, expected "./package.json"' \
    'm.exports = null'
break_manifest "string exports field in manifest is refused" \
    'exports["./package.json"] is missing, expected "./package.json"' \
    'm.exports = "./dist/index.js"'
break_manifest "primitive scripts field in manifest does not crash" \
    'has a "private" field' \
    'm.scripts = "invalid"; m.private = true'

expect_equal "committed package.json exports ./package.json" "./package.json" \
    "$(node -e 'console.log(JSON.parse(require("fs").readFileSync("js/web/web-sdk/package.json")).exports["./package.json"] || "")')"

for entry in "${REQUIRED_FILES[@]}"; do
    break_entries "missing $entry is refused" "missing entry package/$entry" \
        drop "$entry"
done

break_entries "source ts file in package/src is refused" \
    "contains package/src/index.ts: source files must not be in the published package" \
    add "src/index.ts"

break_entries "duplicate wasm in package/src is refused" \
    "contains package/src/generated/fedimint_sdk.wasm: source files must not be in the published package" \
    add "src/generated/fedimint_sdk.wasm"

break_entries "packaging helper script in package/scripts is refused" \
    "contains package/scripts/build.sh: packaging helper scripts must not be in the published package" \
    add "scripts/build.sh"

d="$(valid_dir)"
rewrite "$d/$TARBALL" rewrite_wasm 0
expect_fail "empty wasm module is refused" \
    "$TARBALL: wasm module is empty (0 bytes)" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" rewrite_wasm 50000 0
expect_fail "undersized wasm module is refused" \
    "$TARBALL: wasm module is only 50000 bytes; expected full production build (> 100 KB)" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" rewrite_wasm 120000 1 0
expect_fail "invalid wasm magic bytes is refused" \
    "$TARBALL: wasm module does not start with valid WebAssembly magic bytes (\\0asm)" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" rewrite_wasm 120000 0 1
expect_fail "invalid wasm version header is refused" \
    "$TARBALL: wasm module has an invalid WebAssembly version header (expected version 1)" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" rewrite_wasm 120000 0 0 1
expect_fail "corrupt wasm module body is refused" \
    "$TARBALL: wasm module is not a valid WebAssembly binary" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rm "$d/$TARBALL"
expect_fail "missing tarball is refused" "$TARBALL: not found in $d" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
echo "corrupt gzip stream" >"$d/$TARBALL"
expect_fail "corrupt gzip tarball is refused" "$TARBALL: is not a gzipped tar archive" -- \
    "$PACK" verify "$VERSION" "$d"

d="$(valid_dir)"
rewrite "$d/$TARBALL" drop package.json
expect_fail "tarball missing package.json is refused" "$TARBALL: has no package/package.json" -- \
    "$PACK" verify "$VERSION" "$d"

expect_fail "nonexistent directory is refused" "$WORK/nothing is not a directory" -- \
    "$PACK" verify "$VERSION" "$WORK/nothing"

d="$(valid_dir)"
rewrite "$d/$TARBALL" edit_manifest 'm.private = true; m.version = "9.9.9"'
expect_fail "multiple errors in one tarball are both reported (private)" \
    "$TARBALL: has a \"private\" field" -- "$PACK" verify "$VERSION" "$d"
expect_fail "multiple errors in one tarball are both reported (version)" \
    "$TARBALL: version is \"9.9.9\"" -- "$PACK" verify "$VERSION" "$d"

echo "pack"

cd "$ROOT"
manifest_before="$(cksum js/web/web-sdk/package.json)"
expect_fail "a version outside the grammar is refused" "must be X.Y.Z-beta.N or X.Y.Z" -- \
    "$PACK" 0.1.0-rc.1 "$WORK/never/tarballs"
expect_equal "a refused version leaves no output directory" absent \
    "$([[ -e "$WORK/never" ]] && echo present || echo absent)"
expect_equal "a refused version leaves the manifest as it was" "$manifest_before" \
    "$(cksum js/web/web-sdk/package.json)"

expect_usage "no arguments" -- "$PACK"
expect_usage "a version without an output directory" -- "$PACK" 0.1.0-beta.1
expect_usage "three arguments" -- "$PACK" 0.1.0-beta.1 "$WORK/out" extra
expect_usage "verify without a directory" -- "$PACK" verify 0.1.0-beta.1
expect_usage "verify with four arguments" -- "$PACK" verify 0.1.0-beta.1 "$WORK/out" extra

# Setup throwaway pack tree
pack_tree() {
    local dir web_sdk
    dir="$(mktemp -d "$WORK/tree.XXXX")"
    web_sdk="$dir/js/web/web-sdk"
    mkdir -p "$dir/scripts" "$dir/js/node_modules" "$web_sdk/dist/generated"
    cp "$ROOT/scripts/sdk-components.sh" "$ROOT/scripts/sdk-version.sh" \
        "$ROOT/scripts/web-sdk-pack.sh" "$dir/scripts/"
    fill 0.0.0 "$web_sdk"
    edit_manifest 'm.private = true' "$web_sdk"
    echo "$dir"
}

# Fake pnpm binary that packs directory
mkdir "$WORK/pnpm-bin"
cat >"$WORK/pnpm-bin/pnpm" <<'EOF'
#!/usr/bin/env bash
if [[ $# -ne 5 || "$1" != --dir || "$3" != pack || "$4" != --pack-destination ]]; then
    echo "unexpected pnpm call: $*" >&2
    exit 9
fi
[[ -z "${FAKE_PNPM_FAIL:-}" ]] || exit 1
stage="$(mktemp -d)"
mkdir "$stage/package"
cp -R "$2/." "$stage/package/"
identity="$(node -e '
    const m = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
    console.log(m.name.replace("@", "").replace("/", "-") + "-" + m.version + ".tgz", m.version);
' "$stage/package/package.json")"
read -r tarball version <<<"$identity"
tar -czf "$5/$tarball" -C "$stage" package 2>/dev/null
rm -rf "${stage:?}"
EOF
chmod +x "$WORK/pnpm-bin/pnpm"

tree="$(pack_tree)"
tree_web="$tree/js/web/web-sdk"
tree_pack="$tree/scripts/web-sdk-pack.sh"
cp "$tree_web/package.json" "$WORK/saved-package.json"
out="$WORK/packed/tarballs"
mkdir -p "$out"
echo old >"$out/$TARBALL_BASE-0.0.9.tgz"
echo keep >"$out/notes.txt"

run env PATH="$WORK/pnpm-bin:$PATH" "$tree_pack" "$VERSION" "$out"
expect_equal "a pack of a complete tree succeeds" 0 "$STATUS"
expect_contains "the pack runs verify on the tarball" "$TARBALL: fit to publish" "$OUT"
expect_contains "the report names the tarball" "$TARBALL" "$OUT"
expect_contains "the report has the SHA-256" "$(sha256 "$out/$TARBALL")" "$OUT"
expect_contains "the report has sizes in MB" " MB" "$OUT"
expect_contains "the report lists the wasm module" "fedimint_sdk.wasm" "$OUT"
expect_equal "earlier tarballs are pruned from out dir" "" \
    "$(ls "$out/$TARBALL_BASE-0.0.9.tgz" 2>/dev/null || true)"
expect_equal "other files in out dir are kept" keep "$(cat "$out/notes.txt")"
expect_equal "the tree manifest is restored to 0.0.0 and private: true" same \
    "$(same "$tree_web/package.json" "$WORK/saved-package.json")"
expect_ok "the tarball written passes verify" "$TARBALL: fit to publish" -- \
    "$PACK" verify "$VERSION" "$out"

# Test tarball verification failure restores manifest
tree="$(pack_tree)"
tree_web="$tree/js/web/web-sdk"
tree_pack="$tree/scripts/web-sdk-pack.sh"
cp "$tree_web/package.json" "$WORK/saved-package.json"
add "src/index.ts" "$tree_web"
run env PATH="$WORK/pnpm-bin:$PATH" "$tree_pack" "$VERSION" "$WORK/packed/second"
expect_equal "a pack whose tarball fails verify fails" 1 "$STATUS"
expect_contains "it explains why" "source files must not be in the published package" "$OUT"
expect_equal "manifest is restored after verification failure" same \
    "$(same "$tree_web/package.json" "$WORK/saved-package.json")"

# Test pnpm failure restores manifest
tree="$(pack_tree)"
tree_web="$tree/js/web/web-sdk"
tree_pack="$tree/scripts/web-sdk-pack.sh"
cp "$tree_web/package.json" "$WORK/saved-package.json"
run env PATH="$WORK/pnpm-bin:$PATH" FAKE_PNPM_FAIL=1 "$tree_pack" "$VERSION" "$WORK/packed/third"
expect_equal "a pack whose pnpm fails fails" 1 "$STATUS"
expect_equal "manifest is restored after failed pnpm" same \
    "$(same "$tree_web/package.json" "$WORK/saved-package.json")"

# Test missing installation & missing build outputs
tree="$(pack_tree)"
tree_web="$tree/js/web/web-sdk"
tree_pack="$tree/scripts/web-sdk-pack.sh"
rm -rf "${tree:?}/js/node_modules"
rm "${tree_web:?}/dist/index.js"
rm "${tree_web:?}/dist/generated/fedimint_sdk.wasm"
run env PATH="$WORK/pnpm-bin:$PATH" "$tree_pack" "$VERSION" "$WORK/packed/fourth"
expect_equal "a tree that is not installed or built is refused" 1 "$STATUS"
expect_contains "missing install command named" \
    "js/node_modules: run pnpm --dir js install" "$OUT"
expect_contains "missing build output dist/index.js named" \
    "js/web/web-sdk/dist/index.js: run pnpm --dir js/web/web-sdk run build" "$OUT"
expect_contains "missing build output wasm named" \
    "js/web/web-sdk/dist/generated/fedimint_sdk.wasm: run scripts/nix-build-sdk-wasm.sh" "$OUT"

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
