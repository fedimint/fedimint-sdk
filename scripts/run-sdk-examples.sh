#!/usr/bin/env bash
#
# Stand up a devimint federation of the requested module shape and run the
# fedimint-sdk examples against it.
#
#   scripts/run-sdk-examples.sh [v1|v2] [example ...]
#
# Defaults to shape v2 and to running all four examples: walkthrough, ecash, lightning, onchain.
# v1 stays selectable, but on it the pinned fedimint revision's lnv1 client cannot decode a
# successful send's preimage, so the walkthrough and lightning examples end with an Internal
# error after the payment has actually gone through, while ecash and onchain are unaffected.
#
# Must run inside `nix develop --accept-flake-config .#wasm-tests`: that is the
# only dev shell with devimint, fedimintd, gatewayd, bitcoind, lnd, esplora and
# the recurringd binaries on PATH.

set -euo pipefail

shape="${1:-v2}"
[ $# -gt 0 ] && shift

# shellcheck source=scripts/devimint-shape.sh
. "$(dirname "${BASH_SOURCE[0]}")/devimint-shape.sh" "$shape"

if ! command -v devimint >/dev/null; then
  echo "error: devimint not on PATH; run inside the .#wasm-tests dev shell" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/rust/fedimint-sdk"

# Record what is being driven against what. The flake's devimint and the client
# crates this SDK links are pinned to the same fedimint revision, and the CI
# pins-agree job fails the build if they ever stop being, but printing both here
# is what makes a local tree that has drifted obvious in the log rather than
# mysterious in a test failure.
echo "fedimint-sdk examples: shape=${shape} fed_size=${FM_FED_SIZE}"
devimint --version || true
grep -m1 -o 'rev = "[0-9a-f]\{40\}"' Cargo.toml

# Build before devimint starts, so build time is not charged to the federation's
# uptime and a compile error does not cost a full DKG.
cargo build --locked --examples

devimint wasm-test-setup --exec bash "$repo_root/scripts/run-sdk-examples-inner.sh" "$@"
