#!/usr/bin/env bash
# Runs the multisig tests (tests/multisig.rs) against a dev federation of the Fedi monorepo
# (fedibtc/fedi), which runs the stability pool module the facade needs, at test parameters.
#
# FEDI_REPO points at a checkout of the monorepo. The test binary is built here, in this
# repository's dev shell, and then run under `devi dev-fed` in the monorepo's nix dev shell, which
# builds `devi`, `fedimintd` and `fedimint-cli` and provides the daemons they start.
#
#   FEDI_REPO=/path/to/fedi nix develop --accept-flake-config .#wasm \
#       -c scripts/run-sdk-multisig-tests.sh
set -euo pipefail

if [ -z "${FEDI_REPO:-}" ]; then
  echo "error: set FEDI_REPO to a checkout of the fedibtc/fedi monorepo" >&2
  exit 1
fi
fedi_repo="$(cd "$FEDI_REPO" && pwd)"
crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../rust/fedimint-sdk" && pwd)"

test_bin="$(cd "$crate_dir" && cargo test --locked --test multisig --no-run 2>&1 |
  sed -n 's|^ *Executable tests/multisig.rs (\(.*\))$|\1|p')"
if [ -z "$test_bin" ]; then
  echo "error: could not build the multisig test binary" >&2
  exit 1
fi
# Cargo prints the path relative to the crate when its target directory is the default one, and
# the binary runs from the monorepo.
test_bin="$(cd "$crate_dir" && realpath "$test_bin")"

cd "$fedi_repo"
# The script below runs in the dev shell's own bash, which is where its variables expand.
# shellcheck disable=SC2016
nix develop --accept-flake-config -c bash -c '
set -euo pipefail
cargo build -p devi -p fedi-fedimintd -p fedi-fedimint-cli
source scripts/common.sh
select_federation_modules one
export PATH="$CARGO_BIN_DIR:$PATH"

FM_TEST_DIR="$(mktemp --tmpdir -d fedimint-sdk-multisig-XXXXX)"
export FM_TEST_DIR
export FM_LOGS_DIR="$FM_TEST_DIR/logs"
mkdir -p "$FM_LOGS_DIR"
export FM_FED_SIZE=4
export FM_ADMIN_PASSWORD=p
# The base fee devi gives the mint otherwise is one fedimint 0.13 clients refuse to decode, as the
# monorepo'"'"'s own stability pool tests have it.
export FM_DISABLE_BASE_FEES=1
export FEDI_STABILITY_POOL_V2_MODULE_ENABLE=1
export FEDI_STABILITY_POOL_MODULE_TEST_PARAMS=1
export FM_SDK_REQUIRE_DEVI=1

echo "devi logs: $FM_LOGS_DIR"
devi dev-fed --exec "$1" --test-threads 1
' bash "$test_bin"
