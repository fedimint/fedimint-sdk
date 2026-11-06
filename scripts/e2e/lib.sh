#!/usr/bin/env bash
#
# Shared by scripts/e2e/run.sh and the per-target scripts it sources
# (android.sh, ...): the federation and shape checks, choosing tests, the lnv2
# gateway pin, Appium, and exit hooks. Platform-neutral; nothing here touches
# a device.

REPO_ROOT=$(git rev-parse --show-toplevel)
E2E_PKG="$REPO_ROOT/js/e2e"

# ── Exit hooks ────────────────────────────────────────────────────────────
# Run in reverse order of registration on any exit, so a target can undo what
# it set up (a log capture, a device property) whatever ends the run.
E2E_EXIT_HOOKS=()
e2e_on_exit() { E2E_EXIT_HOOKS+=("$1"); }
e2e_run_exit_hooks() {
  local i
  for ((i = ${#E2E_EXIT_HOOKS[@]} - 1; i >= 0; i--)); do
    eval "${E2E_EXIT_HOOKS[$i]}" || true
  done
}
trap e2e_run_exit_hooks EXIT

# ── The federation ────────────────────────────────────────────────────────

# Every test runs against devimint: there is no federation-free mode, so what
# CI runs and what a contributor runs are the same thing.
e2e_require_federation() {
  if [[ -n "${FM_FEDERATION_BASE_PORT:-}" ]]; then return; fi
  cat >&2 <<'MSG'
No federation in the environment (FM_FEDERATION_BASE_PORT is unset).
This suite runs inside devimint. Start it with:
  just test-e2e <target> [v1|v2]
or, by hand, inside the target's nix shell:
  scripts/setup_test_shell.sh bash scripts/e2e/run.sh <target>
MSG
  exit 1
}

# scripts/setup_test_shell.sh exports the shape it started the federation
# with. `mixed` is refused: the SDK rejects a federation that mixes module
# generations by design, so there is no successful payment to test.
e2e_require_shape() {
  case "${FM_SDK_SHAPE:-}" in
    v1 | v2) ;;
    mixed)
      echo "FM_SDK_SHAPE=mixed is not an e2e target: the SDK refuses federations" \
        "that mix module generations. Use v1 or v2." >&2
      exit 1
      ;;
    *)
      echo "FM_SDK_SHAPE is '${FM_SDK_SHAPE:-}' — expected v1 or v2. Start the run through" \
        "scripts/setup_test_shell.sh, which sets it." >&2
      exit 1
      ;;
  esac
}

# TESTS_TO_RUN: space-separated test names, or `all`. CI runs everything;
# interactively, ask.
e2e_choose_tests() {
  if [[ -n "${TESTS_TO_RUN:-}" ]]; then return; fi
  if [[ "${CI:-}" == "true" ]]; then
    TESTS_TO_RUN=all
    return
  fi
  echo "Which tests to run? (names from js/e2e/src/registry.ts, or all) [all]"
  read -r TESTS_TO_RUN
  TESTS_TO_RUN=${TESTS_TO_RUN:-all}
}

# ── lnv2: one gateway, the funded one ─────────────────────────────────────
#
# The lnv2 client picks a gateway at random from the guardians' lists, and
# devimint's wasm-test-setup funds only the LND gateway, so a receive through
# an LDK gateway cannot be funded and a send through the faucet's own LDK
# gateway to the faucet's invoice is a self-payment. Removing the other
# entries, through the admin call devimint used to add them, leaves the funded
# gateway as the only choice — as on v1, where the SDK picks the cheapest and
# wasm-test-setup sets LND's fees to zero. The same step as
# rust/fedimint-sdk/tests/integration.rs's pin_lnv2_gateway_to_lnd, once per
# guardian, since the client reads every guardian's list.
#
# `--peer` asks each guardian for its own list: without it `list` answers with
# the union of the first three to reply, which can hide the fourth's entries.
# FM_MINT_CLIENT is a ready-to-run command line, deliberately unquoted so it
# splits into a program and its arguments. `pass` is devimint's admin password.
pin_lnv2_gateways() {
  [[ "$FM_SDK_SHAPE" == v2 ]] || return 0
  local fed_size="${FM_FED_SIZE:-4}"
  local lnd_needle=":${FM_PORT_GW_LND:?FM_PORT_GW_LND is not set}/"
  local peer listed url remaining
  for ((peer = 0; peer < fed_size; peer++)); do
    # shellcheck disable=SC2086
    listed="$($FM_MINT_CLIENT --our-id "$peer" --password pass module lnv2 gateways list --peer "$peer")"
    if ! grep -qF "$lnd_needle" <<<"$listed"; then
      echo "Guardian $peer does not list the LND gateway among its lnv2 gateways: $listed" >&2
      exit 1
    fi
    while IFS= read -r url; do
      [[ "$url" == *"$lnd_needle"* ]] && continue
      echo "guardian $peer: removing lnv2 gateway not on LND: $url"
      # shellcheck disable=SC2086
      $FM_MINT_CLIENT --our-id "$peer" --password pass module lnv2 gateways remove "$url" >/dev/null
    done < <(grep -o '"http[^"]*"' <<<"$listed" | tr -d '"')
    # Read back from the same guardian: a removal that did nothing fails the
    # run here rather than as a refunded payment later.
    # shellcheck disable=SC2086
    listed="$($FM_MINT_CLIENT --our-id "$peer" --password pass module lnv2 gateways list --peer "$peer")"
    remaining="$(grep -o '"http[^"]*"' <<<"$listed" | grep -vF "$lnd_needle" || true)"
    if [[ -n "$remaining" ]]; then
      echo "Guardian $peer still lists lnv2 gateways not on LND after the pin: $remaining" >&2
      exit 1
    fi
  done
}

# ── Appium ────────────────────────────────────────────────────────────────

# Starts Appium with `driver` (uiautomator2, xcuitest) and exports
# APPIUM_PORT. It is stopped on exit: it runs with shell access to the device,
# and neither a developer machine nor a self-hosted runner goes away with the
# run.
e2e_start_appium() {
  export APPIUM_HOME="${APPIUM_HOME:-$E2E_PKG/.appium}"
  e2e_on_exit e2e_stop_appium
  bash "$REPO_ROOT/scripts/e2e/appium.sh" "$1"
  APPIUM_PORT=$(cat "$APPIUM_HOME/appium_port.txt")
  export APPIUM_PORT
  echo "Appium is serving on port $APPIUM_PORT"
}

e2e_stop_appium() {
  local pid_file="$APPIUM_HOME/appium_pid.txt"
  if [[ -f "$pid_file" ]]; then
    kill "$(cat "$pid_file")" 2>/dev/null || true
    rm -f "$pid_file" "$APPIUM_HOME/appium_port.txt"
  fi
}
