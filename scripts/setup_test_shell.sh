#!/usr/bin/env bash

set -euo pipefail

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
