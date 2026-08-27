#!/bin/sh

set -e 

echo "Building WASM bundle..."
nix build -L .#wasmBundle

echo "Copying WASM files..."
cp result/share/fedimint-client-wasm/fedimint_* web/wasm-bundler/
cp result/share/fedimint-client-wasm-web/fedimint_* web/wasm-web/

# Lets future builds replace the existing files
chmod u+w web/wasm-bundler/fedimint_*
chmod u+w web/wasm-web/fedimint_*