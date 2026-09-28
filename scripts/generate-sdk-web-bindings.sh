#!/usr/bin/env bash
#
# Generates js/web/sdk-web's TypeScript bindings from an already built rust/fedimint-sdk wasm
# module, and stages that module beside them.
#
#   scripts/generate-sdk-web-bindings.sh [fedimint_sdk.wasm]
#     -> js/web/sdk-web/src/generated/{index.ts,fedimint_sdk.ts,fedimint_sdk-ffi.ts,
#                                       fedimint_sdk_bg.js,*.d.ts,fedimint_sdk.wasm}
#
# The module defaults to the Nix build (scripts/nix-build-sdk-wasm.sh), which is what CI reads
# and what the committed TypeScript must match: `wasm-bindgen`'s glue depends on which imports
# the module still has after optimisation, so a debug module produces a different
# `fedimint_sdk_bg.js`. ubrn reads the UniFFI metadata out of the `.wasm` itself (its `wasm2`
# flavor), so the TypeScript can never describe a module other than the one the browser loads.
#
# ubrn finds the module through `cargo metadata`'s target directory, so the given file is laid
# out under a throwaway CARGO_TARGET_DIR and ubrn is told not to build. Run inside the
# `.#wasm` shell: it needs `ubrn`, `wasm-bindgen` 0.2.106 and `cargo` on PATH, and
# `pnpm install` done in js/: ubrn formats its own output with the workspace's prettier, and
# this script's last step runs that same prettier over the wasm-bindgen glue ubrn leaves alone.
#
# `wasm-bindgen` 0.2.106 orders the closure exports in `fedimint_sdk_bg.js` and
# `fedimint_sdk_bg.wasm.d.ts` differently on every invocation, so those two files churn on
# regeneration even with no meaningful change (`fedimint_sdk_bg.d.ts`, the third glue file, is
# a fixed stub); the three ubrn-written TypeScript files (`index.ts`, `fedimint_sdk.ts`,
# `fedimint_sdk-ffi.ts`) are byte-stable, and CI's freshness check compares only those.
#
# The staged module is the one the browser downloads, so it is optimised for size last:
# `wasm-opt -Oz` shrinks the code and strips the `name` section (the debug function names, a
# third of the file as the compiler emits it). It runs after ubrn and wasm-bindgen because both
# read the module as rustc wrote it, and neither optimises it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/js/web/sdk-web"
WASM="${1:-$("$ROOT/scripts/nix-build-sdk-wasm.sh")}"

if [[ ! -f "$WASM" ]]; then
  echo "no wasm module at $WASM" >&2
  echo "build one first: scripts/nix-build-sdk-wasm.sh (or pass its path as the argument)" >&2
  exit 1
fi
for tool in ubrn wasm-bindgen wasm-opt cargo; do
  command -v "$tool" >/dev/null ||
    { echo "$tool not on PATH; run in the .#wasm shell" >&2; exit 1; }
done

TARGET="$(mktemp -d)"
trap 'rm -rf "$TARGET"' EXIT
mkdir -p "$TARGET/wasm32-unknown-unknown/release"
cp "$WASM" "$TARGET/wasm32-unknown-unknown/release/fedimint_sdk.wasm"
chmod u+w "$TARGET/wasm32-unknown-unknown/release/fedimint_sdk.wasm"

echo "==> Generating $PKG/src/generated from $WASM"
rm -rf "$PKG/src/generated"
cd "$PKG"
CARGO_TARGET_DIR="$TARGET" ubrn build wasm2 --config ubrn.config.yaml --release --no-wasm-build

echo "==> Formatting wasm-bindgen glue with prettier"
pnpm --dir "$ROOT/js" exec prettier --write \
  "$PKG/src/generated/fedimint_sdk_bg.js" \
  "$PKG/src/generated/fedimint_sdk_bg.d.ts" \
  "$PKG/src/generated/fedimint_sdk_bg.wasm.d.ts"

echo "==> Optimising the staged module for size with wasm-opt"
STAGED="$PKG/src/generated/fedimint_sdk.wasm"
wasm-opt -Oz -o "$STAGED.opt" "$STAGED"
mv "$STAGED.opt" "$STAGED"

echo "==> Done."
ls -la "$PKG/src/generated"
