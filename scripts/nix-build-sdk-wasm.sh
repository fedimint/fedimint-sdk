#!/usr/bin/env bash
#
# Builds rust/fedimint-sdk for the browser via Nix and prints the module's path.
#
#   scripts/nix-build-sdk-wasm.sh
#     -> prints <store path>/lib/fedimint_sdk.wasm
#
# This is the module-only half, no bindings: the `.wasm` is the input the web binding generator
# reads (scripts/generate-sdk-web-bindings.sh), and building it here means iterating on the
# bindings needs no Rust rebuild. Nothing compiles if the Cachix cache is warm.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

out="$(nix build "$ROOT#fedimint-sdk-wasm" --accept-flake-config --no-link --print-out-paths)"
echo "$out/lib/fedimint_sdk.wasm"
