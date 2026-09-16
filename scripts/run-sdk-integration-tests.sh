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

# shellcheck source=scripts/devimint-shape.sh
. "$(dirname "${BASH_SOURCE[0]}")/devimint-shape.sh" "$shape"

# Turn a missing federation into a failure rather than a skipped test.
export FM_SDK_REQUIRE_DEVIMINT=1

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

# devimint integration tests can be flaky in constrained CI environments (e.g. timeouts).
# Retry up to 3 times to ensure flakiness doesn't fail the build.
MAX_RETRIES=3
if [ -z "${FM_TEST_DIR:-}" ]; then
  BASE_TEST_DIR="$(mktemp -d)"
else
  BASE_TEST_DIR="$FM_TEST_DIR"
fi

for ((i=1; i<=MAX_RETRIES; i++)); do
  echo "Running integration tests (Attempt $i of $MAX_RETRIES)..."

  # Devimint uses the exact path in FM_TEST_DIR. We must give each attempt
  # a completely clean directory, otherwise bitcoind wallet state from the
  # failed attempt will corrupt the retry.
  export FM_TEST_DIR="$BASE_TEST_DIR/attempt-$i"
  mkdir -p "$FM_TEST_DIR"
  export TMPDIR="$FM_TEST_DIR"

  rc=0
  devimint wasm-test-setup --exec cargo test --locked --test integration "$@" || rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "Tests passed on attempt $i"
    exit 0
  fi
  echo "Attempt $i failed with exit code $rc."
  if [ "$i" -lt "$MAX_RETRIES" ]; then
    echo "Retrying in 10 seconds..."
    sleep 10
  fi
done
echo "Tests failed after $MAX_RETRIES attempts."
exit 1
