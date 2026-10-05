#!/usr/bin/env bash

set -euo pipefail

# The federation's module shape: v1 (mint, wallet, ln) unless the caller asks for
# another through FM_SDK_SHAPE. fedimint defaults to the v2 modules, and this SDK
# refuses a federation that mixes generations, so the flags are always spelled out
# for exactly one shape — the same ones scripts/devimint-shape.sh uses for the Rust
# integration tests. A caller's FM_SDK_SHAPE is honoured rather than overwritten:
# the Android suite runs both v1 and v2 through this script (see
# js/android/integration-tests/README.md), the wasm suite keeps the v1 default.
# shellcheck source=scripts/devimint-modules.sh
. "$(dirname "${BASH_SOURCE[0]}")/devimint-modules.sh" "${FM_SDK_SHAPE:-v1}"
echo "devimint federation shape: ${FM_SDK_SHAPE}" \
  "(mint=${FM_ENABLE_MODULE_MINT} wallet=${FM_ENABLE_MODULE_WALLET} ln=${FM_ENABLE_MODULE_LNV1}" \
  "mintv2=${FM_ENABLE_MODULE_MINTV2} walletv2=${FM_ENABLE_MODULE_WALLETV2} lnv2=${FM_ENABLE_MODULE_LNV2})"

# devimint now allocates a free faucet port per run and fails the setup if it cannot
# bind it, so concurrent runs no longer collide (see issue #340). Only a port pinned
# through FM_FAUCET_PORT can still be occupied by a stale or concurrent devimint, so
# wait for that one, and fail fast with a diagnostic if it never frees up. (devimint
# reports whichever port it ended up with as FM_PORT_FAUCET, which is what the tests
# read; that one is set by devimint itself, not by whoever runs this script.)
faucet_port="${FM_FAUCET_PORT:-}"

# The tests fetch from `localhost`, which may resolve to either loopback address, so a
# squatter on either one counts as occupied. `timeout` bounds a probe that would
# otherwise stall on firewalled/DROPped packets.
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
    if command -v ss >/dev/null; then
      ss -ltnp "sport = :${faucet_port}" >&2 || true
    fi
    exit 1
  fi
  echo "faucet port ${faucet_port} is in use, waiting for it to be free..."
  sleep 5
done

# Even though it would be better, do not use `exec` for this,
# as pnpm seems to be swallowing non-zero error codes if it
# is run like this, but only when this whole script is called from
# another pnpm instance.
devimint wasm-test-setup --exec "$@"
