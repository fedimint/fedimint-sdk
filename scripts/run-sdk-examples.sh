#!/usr/bin/env bash
#
# Stand up a devimint federation of the requested module shape and run the
# fedimint-sdk examples against it.
#
#   scripts/run-sdk-examples.sh [v1|v2] [example|shell ...]
#
# The shape is optional and defaults to v2; every other argument (including a leading one that is
# not v1 or v2) is an example name. Defaults to running all four examples: walkthrough, ecash,
# lightning, onchain. The name `shell` instead hands the federation to an interactive shell for
# running the examples by hand. v1 stays selectable, but on it the pinned fedimint revision's lnv1
# client cannot decode a successful send's preimage, so the walkthrough and lightning examples end
# with an Internal error after the payment has actually gone through, while ecash and onchain are
# unaffected.
#
# Runs inside the .#wasm-tests dev shell, the only one with devimint, fedimintd,
# gatewayd, bitcoind, lnd, esplora and the recurringd binaries on PATH, and
# enters it itself when started from outside it, so a plain shell will do.

set -euo pipefail

shape=v2
if [ $# -gt 0 ] && { [ "$1" = v1 ] || [ "$1" = v2 ]; }; then
  shape="$1"
  shift
fi

# shellcheck source=scripts/devimint-shape.sh
. "$(dirname "${BASH_SOURCE[0]}")/devimint-shape.sh" "$shape"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# devimint missing from PATH means this is a plain shell: re-run this script, with the same
# arguments, inside the dev shell that has the federation's binaries. In there devimint is on
# PATH and this branch is skipped, so the re-run cannot loop.
if ! command -v devimint >/dev/null; then
  exec nix develop --accept-flake-config "$repo_root#wasm-tests" \
    --command "$repo_root/scripts/run-sdk-examples.sh" "$shape" "$@"
fi
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
