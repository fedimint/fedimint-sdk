#!/bin/sh

set -e 

# Invoked from the js/ workspace root; the flake and the copy targets are
# relative to the repo root.
cd "$(dirname "$0")/.."

echo "Building WASM bundle..."
nix build -L .#wasmBundle

echo "Copying WASM files..."
cp result/share/fedimint-client-wasm/fedimint_* js/web/wasm-bundler/
cp result/share/fedimint-client-wasm-web/fedimint_* js/web/wasm-web/

# Lets future builds replace the existing files
chmod u+w js/web/wasm-bundler/fedimint_*
chmod u+w js/web/wasm-web/fedimint_*