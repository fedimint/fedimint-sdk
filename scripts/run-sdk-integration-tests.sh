#!/usr/bin/env bash
#
# Stand up a devimint federation of the requested module shape and run the
# fedimint-sdk integration tests against it.
#
#   scripts/run-sdk-integration-tests.sh [v1|v2|mixed] [extra cargo test args...]
#
# Must run inside `nix develop --accept-flake-config .#wasm-tests`: that is the
# only dev shell with devimint, fedimintd, gatewayd, bitcoind, lnd, esplora and
# the recurringd binaries on PATH.

set -euo pipefail

shape="${1:-v1}"
[ $# -gt 0 ] && shift

# fedimintd decides the module set from these; devimint only passes the
# environment through. The v1 modules default off and the v2 modules default on.
# Every variable is spelled out as 0 or 1 rather than left to a default, because
# fedimintd and devimint disagree about what an unrecognised value means:
# fedimintd's module servers read it through is_env_var_set_opt and fall back to
# their own default, while devimint's own supports_* helpers read it through
# is_env_var_set and assume "on". A typo would give a federation of one shape and
# a harness expecting another.
case "$shape" in
  v1)
    export FM_ENABLE_MODULE_MINT=1 FM_ENABLE_MODULE_WALLET=1 FM_ENABLE_MODULE_LNV1=1
    export FM_ENABLE_MODULE_MINTV2=0 FM_ENABLE_MODULE_WALLETV2=0 FM_ENABLE_MODULE_LNV2=0
    ;;
  v2)
    export FM_ENABLE_MODULE_MINT=0 FM_ENABLE_MODULE_WALLET=0 FM_ENABLE_MODULE_LNV1=0
    export FM_ENABLE_MODULE_MINTV2=1 FM_ENABLE_MODULE_WALLETV2=1 FM_ENABLE_MODULE_LNV2=1
    ;;
  mixed)
    # Mix the lightning generations only. Mixing mint or wallet generations
    # instead breaks devimint's own gateway peg-in ("Polling gateway pegin
    # claim failed"), whereas ln + lnv2 side by side is the configuration
    # fedimint's own wasm test runs.
    export FM_ENABLE_MODULE_MINT=1 FM_ENABLE_MODULE_WALLET=1 FM_ENABLE_MODULE_LNV1=1
    export FM_ENABLE_MODULE_MINTV2=0 FM_ENABLE_MODULE_WALLETV2=0 FM_ENABLE_MODULE_LNV2=1
    ;;
  *)
    echo "usage: $0 [v1|v2|mixed] [cargo test args...]" >&2
    exit 2
    ;;
esac
export FM_SDK_SHAPE="$shape"

# Turn a missing federation into a failure rather than a skipped test.
export FM_SDK_REQUIRE_DEVIMINT=1

# The federation is configured with the WebSocket API only, the same choice
# fedimint's own wasm test makes. FM_ENABLE_IROH already resolves to false under
# devimint, because fedimintd defaults it to !is_running_in_test_env(); this
# second switch is what also suppresses the transitional Iroh 1.0 endpoint,
# which is on by default in every environment.
export FM_IROH_NEXT_ENABLE=false

# One guardian rather than devimint's default of four (devimint/src/cli.rs:53).
# DKG and every per-guardian admin call scale with this, and upstream runs
# `devimint -n 1` for its own CLI tests. Override to 4 to match the JS suite's
# federation.
export FM_FED_SIZE="${FM_FED_SIZE:-1}"

if ! command -v devimint >/dev/null; then
  echo "error: devimint not on PATH; run inside the .#wasm-tests dev shell" >&2
  exit 1
fi

# devimint allocates a free faucet port per run and reports it back as
# FM_PORT_FAUCET, so concurrent runs no longer collide. Only a port pinned
# through FM_FAUCET_PORT can still be occupied by a stale devimint, so wait for
# that one and fail fast with a diagnostic if it never frees up.
faucet_port="${FM_FAUCET_PORT:-}"

faucet_port_in_use() {
  local host
  for host in 127.0.0.1 ::1; do
    if timeout 2 bash -c "exec 3<>/dev/tcp/${host}/${faucet_port}" 2>/dev/null; then
      return 0
    fi
  done
  return 1
}

deadline=$((SECONDS + 120))
while [ -n "$faucet_port" ] && faucet_port_in_use; do
  if ((SECONDS >= deadline)); then
    echo "error: faucet port ${faucet_port} is still in use;" \
      "is a stale or concurrent devimint running on this machine?" >&2
    command -v ss >/dev/null && ss -ltnp "sport = :${faucet_port}" >&2 || true
    exit 1
  fi
  echo "faucet port ${faucet_port} is in use, waiting for it to be free..."
  sleep 5
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/rust/fedimint-sdk"

# Record what is being driven against what. The flake's devimint and the client
# crates this SDK links are pinned to the same fedimint revision, and the CI
# pins-agree job fails the build if they ever stop being, but printing both here
# is what makes a local tree that has drifted obvious in the log rather than
# mysterious in a test failure.
echo "fedimint-sdk integration tests: shape=${shape} fed_size=${FM_FED_SIZE}"
devimint --version || true
grep -m1 -o 'rev = "[0-9a-f]\{40\}"' Cargo.toml

# Compile before devimint starts, so build time is not charged to the
# federation's uptime and a compile error does not cost a full DKG.
cargo test --locked --test integration --no-run

devimint wasm-test-setup --exec cargo test --locked --test integration "$@"
