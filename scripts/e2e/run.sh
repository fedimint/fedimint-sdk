#!/usr/bin/env bash
#
#   scripts/e2e/run.sh <target>          target: android | ios | web
#
# Runs the cross-platform SDK suite (js/e2e) on one target, against the
# devimint federation scripts/setup_test_shell.sh started around it:
#
#   just test-e2e android v2
#   # or, inside the target's nix shell:
#   FM_SDK_SHAPE=v2 scripts/setup_test_shell.sh bash scripts/e2e/run.sh android
#
# The target's script (scripts/e2e/<target>.sh) provisions the device and the
# harness; the runner (js/e2e/src/runner.ts) then drives it. TESTS_TO_RUN
# picks tests (names, or `all`); CI runs all, otherwise it asks. Screenshots,
# UI trees and logs land in js/e2e/.artifacts/<target>-<shape>.
set -euo pipefail

E2E_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/e2e/lib.sh
source "$E2E_DIR/lib.sh"

TARGET="${1:-}"
case "$TARGET" in
  android | ios | web) ;;
  *)
    echo "usage: $0 <android|ios|web>" >&2
    exit 1
    ;;
esac

e2e_require_federation
e2e_require_shape
echo "=== SDK e2e on $TARGET — federation shape $FM_SDK_SHAPE ==="

export ARTIFACTS_DIR="$E2E_PKG/.artifacts/$TARGET-$FM_SDK_SHAPE"
rm -rf "$ARTIFACTS_DIR"
mkdir -p "$ARTIFACTS_DIR"

if [[ ! -x "$REPO_ROOT/js/node_modules/.bin/ts-node" ]]; then
  echo "Installing workspace deps..."
  pnpm --dir "$REPO_ROOT/js" install --frozen-lockfile
fi

# shellcheck source=/dev/null
source "$E2E_DIR/$TARGET.sh"
provision
e2e_choose_tests
pin_lnv2_gateways

echo "Running on $TARGET: $TESTS_TO_RUN"
cd "$E2E_PKG"
# FAUCET is read on the host, so it keeps the host's port. Word-split on
# purpose: TESTS_TO_RUN is the runner's argument list.
# shellcheck disable=SC2086
E2E_TARGET="$TARGET" \
  FAUCET="${FAUCET:-http://localhost:${FM_PORT_FAUCET:-15243}}" \
  pnpm exec ts-node --project tsconfig.json src/runner.ts $TESTS_TO_RUN
