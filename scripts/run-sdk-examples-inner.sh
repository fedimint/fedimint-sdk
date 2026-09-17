#!/usr/bin/env bash
#
# devimint's --exec target for scripts/run-sdk-examples.sh; do not run this directly. The
# examples know nothing about devimint, so this script is devimint's side of the table: it
# stands up a data directory per wallet, starts an example, reads the line it printed for a
# counterparty to act on, and plays that counterparty (devimint's faucet, its bitcoind) through
# the environment devimint's wasm-test-setup exports (FM_CLIENT_DIR, FM_PORT_FAUCET,
# FM_BTC_CLIENT and the rest). It assumes the examples are already built.
#
#   scripts/run-sdk-examples-inner.sh [example|shell ...]
#
# Defaults to running all four examples in order: walkthrough, lightning, ecash, onchain. The
# name `shell` instead hands the federation to the user's own shell, for running the examples
# by hand, and tears it down when that shell exits.

set -euo pipefail

# How long grab (below) waits for an example to print the line it is polling for, in seconds.
grab_timeout=120

root="$(mktemp -d)"

# Kills any background example still running when the script exits, because grab below gave
# up without ever seeing the process die, so a failed run does not leave a stray cargo process
# behind waiting on a payment that is never coming; then removes the temporary root.
cleanup() {
  local j
  for j in $(jobs -p); do
    kill "$j" 2>/dev/null || true
  done
  rm -rf "$root"
}
trap cleanup EXIT

wallet_a="$root/a"
wallet_b="$root/b"

invite="$(cat "$FM_CLIENT_DIR/invite-code")"

if [ "${FM_SDK_SHAPE:-}" = v2 ]; then
  # On the v2 shape, leave the LND gateway as the guardian's only lnv2 gateway. The lnv2 client
  # picks a gateway at random from the guardians' list on purpose, and devimint's
  # wasm-test-setup funds only the LND gateway's ecash, so a receive through either LDK gateway
  # cannot be funded and a send through the faucet's own LDK gateway to the faucet's invoice is
  # a self-payment. Removing the two LDK entries through the same admin call devimint used to
  # add them makes the SDK's choice the funded gateway, and leaves the faucet's LDK node as the
  # counterparty, as on the v1 shape.
  #
  # --our-id 0: the harness always runs a single guardian (FM_FED_SIZE=1). pass is the admin
  # password devimint sets everywhere.
  #
  # FM_MINT_CLIENT is a ready-to-run command line, so it is deliberately left unquoted below:
  # the expansion splits it into a program and its arguments on whitespace, like the tests'
  # Rust harness does with `split_whitespace`. Bash's unquoted expansion also glob-expands
  # each word, but devimint's client command line never contains a glob character, so that
  # extra step changes nothing here.
  # shellcheck disable=SC2086
  listed="$($FM_MINT_CLIENT --our-id 0 --password pass module lnv2 gateways list)"
  lnd_needle=":${FM_PORT_GW_LND}/"
  while IFS= read -r url; do
    case "$url" in
      *"$lnd_needle"*) continue ;;
    esac
    echo "removing lnv2 gateway not on LND: $url"
    # shellcheck disable=SC2086
    $FM_MINT_CLIENT --our-id 0 --password pass module lnv2 gateways remove "$url"
  done < <(grep -o '"http[^"]*"' <<<"$listed" | tr -d '"')
fi

# Reports which example failed and stops the run: every failure below goes through this, so it
# is always the example's name that ends up in the message, never a bare cargo or curl error.
die() {
  echo "error: example '$1' failed" >&2
  exit 1
}

# Runs <name>'s cargo binary to completion in the foreground with the remaining arguments,
# its combined output tee'd to <log> and to the terminal.
run_example() {
  local name="$1" log="$2"
  shift 2
  cargo run --locked --example "$name" -- "$@" 2>&1 | tee "$log" || die "$name"
}

# Runs <name>'s cargo binary in the background with the remaining arguments, its combined
# output redirected to <log>, and sets the global $pid to the backgrounded process's pid.
#
# $pid is set here rather than printed for a caller to capture with $(...): a pid a command
# substitution's subshell backgrounds stops being this script's child once that subshell exits,
# and `wait` only works on a direct child, so start_example has to run in this shell, not a
# child of it.
start_example() {
  local name="$1" log="$2"
  shift 2
  cargo run --locked --example "$name" -- "$@" >"$log" 2>&1 &
  pid=$!
}

# Waits for <pid> (a pid start_example returned), prints the example's output from <log> once it
# has ended, since a backgrounded example writes only there, and stops the run, naming <name>,
# if the example exited with a failure.
finish() {
  local name="$1" pid="$2" log="$3"
  local status=0
  wait "$pid" || status=$?
  cat "$log"
  [ "$status" -eq 0 ] || die "$name"
}

# Polls <log> once a second, up to $grab_timeout seconds, for the first line starting with
# "<key> " and prints what follows it on that line. A pid, when given, is checked between
# polls: once that process is no longer running the line is never coming, so grab gives up
# instead of waiting out the rest of the timeout on an example that already failed.
grab() {
  local log="$1" key="$2" pid="${3:-}"
  local waited=0
  while [ "$waited" -lt "$grab_timeout" ]; do
    if [ -f "$log" ]; then
      local line
      line="$(grep -m1 "^$key " "$log" || true)"
      if [ -n "$line" ]; then
        printf '%s\n' "${line#"$key" }"
        return 0
      fi
    fi
    if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  return 1
}

# One call to devimint's faucet, a lightning node outside the federation: posts <data> to its
# <endpoint> (`pay` with a bolt11 invoice to pay it, `invoice` with an msat amount to be
# issued one) and fails on an HTTP error.
faucet() {
  local endpoint="$1" data="$2"
  curl -sS --fail-with-body -X POST --data "$data" "http://localhost:$FM_PORT_FAUCET/$endpoint"
}

# Pays a bolt11 invoice from outside the federation: the counterparty for a receive. Fails the
# run through die, naming <name>, if the faucet call fails.
pay_invoice() {
  local name="$1" invoice="$2"
  faucet pay "$invoice" >/dev/null || die "$name"
}

# Issues a bolt11 invoice for <msats> millisatoshis from outside the federation, printed for
# the caller to pass to a send. Fails the run through die, naming <name>, if the faucet call
# fails.
new_invoice() {
  local name="$1" msats="$2"
  faucet invoice "$msats" || die "$name"
}

# Runs one bitcoin-cli command against devimint's regtest node, on the wallet devimint funds
# for itself; FM_BTC_CLIENT is left unquoted so its own words split the way devimint hands
# them out, the same convention FM_MINT_CLIENT uses above.
bitcoin_cli() {
  # shellcheck disable=SC2086
  $FM_BTC_CLIENT -rpcwallet=default "$@"
}

# Sends <sats> satoshis to <address> from bitcoind's own wallet, converting to the BTC amount
# bitcoin-cli expects.
send_to_address() {
  local address="$1" sats="$2"
  local btc
  btc="$(printf '%d.%08d' $((sats / 100000000)) $((sats % 100000000)))"
  bitcoin_cli sendtoaddress "$address" "$btc"
}

# Mines <n> blocks to a fresh address, confirming whatever was just paid into the federation.
mine_blocks() {
  local address
  address="$(bitcoin_cli getnewaddress)"
  bitcoin_cli generatetoaddress "$1" "$address"
}

# The crate doc's walkthrough: fund the wallet's first invoice, then let the example send the
# faucet invoice generated below and finish the rest (ecash send, reattach, activity) on its own.
run_walkthrough() {
  local log="$root/walkthrough.log"
  local invoice
  invoice="$(new_invoice walkthrough 50000)"
  start_example walkthrough "$log" "$wallet_a" "$invite" "$invoice"
  local fund
  fund="$(grab "$log" "invoice:" "$pid")" || die walkthrough
  pay_invoice walkthrough "$fund"
  finish walkthrough "$pid" "$log"
  # The walkthrough pays the invoice handed to it above and then sends ecash of its own;
  # both are visible in its log if it actually ran to completion.
  grep -q '^state: paid, ' "$log" || die walkthrough
  grep -q '^notes: ' "$log" || die walkthrough
}

# Receives a payment, then sends a fresh faucet invoice back out.
run_lightning() {
  local receive_log="$root/lightning-receive.log"
  start_example lightning "$receive_log" "$wallet_a" "$invite" receive 200000 "an example"
  local invoice
  invoice="$(grab "$receive_log" "invoice:" "$pid")" || die lightning
  pay_invoice lightning "$invoice"
  finish lightning "$pid" "$receive_log"
  grep -q '^state: Claimed$' "$receive_log" || die lightning

  local send_log="$root/lightning-send.log"
  local send_invoice
  send_invoice="$(new_invoice lightning 50000)"
  run_example lightning "$send_log" "$wallet_a" "$invite" send "$send_invoice"
  grep -q '^state: paid, ' "$send_log" || die lightning
}

# The ecash example has no subcommand of its own that funds a wallet, so it is funded the same
# way the lightning receive above is: any receive that credits the balance will do. Wallet a
# then sends notes that wallet b redeems, and afterwards asks to cancel that same send, which
# settles it as redeemed because the receiver got there first.
run_ecash() {
  local fund_log="$root/ecash-fund.log"
  start_example lightning "$fund_log" "$wallet_a" "$invite" receive 200000 "an example"
  local invoice
  invoice="$(grab "$fund_log" "invoice:" "$pid")" || die ecash
  pay_invoice ecash "$invoice"
  finish ecash "$pid" "$fund_log"
  grep -q '^state: Claimed$' "$fund_log" || die ecash

  local send_log="$root/ecash-send.log"
  run_example ecash "$send_log" "$wallet_a" "$invite" send 50000
  local notes operation
  notes="$(grab "$send_log" "notes:")" || die ecash
  operation="$(grab "$send_log" "operation:")" || die ecash

  local receive_log="$root/ecash-receive.log"
  run_example ecash "$receive_log" "$wallet_b" "$invite" receive "$notes"
  grep -q '^state: Done$' "$receive_log" || die ecash

  local cancel_log="$root/ecash-cancel.log"
  run_example ecash "$cancel_log" "$wallet_a" "$invite" cancel "$operation"
  grep -q '^state: Redeemed$' "$cancel_log" || die ecash
}

# Pays a deposit address and mines it to confirmation, then withdraws part of it back out.
run_onchain() {
  local receive_log="$root/onchain-receive.log"
  start_example onchain "$receive_log" "$wallet_a" "$invite" receive
  local address
  address="$(grab "$receive_log" "address:" "$pid")" || die onchain
  send_to_address "$address" 100000
  mine_blocks 21
  finish onchain "$pid" "$receive_log"
  grep -q '^state: Claimed' "$receive_log" || die onchain

  local send_log="$root/onchain-send.log"
  run_example onchain "$send_log" "$wallet_a" "$invite" \
    send "$(bitcoin_cli getnewaddress)" 20000
  grep -q '^state: Succeeded' "$send_log" || die onchain
}

# Hands the federation to the user's own shell instead of an example: FM_SDK_SHELL, which the
# outer script records from SHELL before nix develop replaces it. The invite code and a fresh
# wallet directory are in its environment, and the counterparty helpers above are on its PATH
# as commands, since functions cannot be handed to a shell that is not bash: each command is a
# script carrying the function definitions and calling one of them. The federation stays up
# until the shell exits.
run_shell() {
  local bin="$root/bin"
  local helper
  mkdir -p "$bin"
  for helper in faucet bitcoin_cli send_to_address mine_blocks; do
    {
      echo '#!/usr/bin/env bash'
      echo 'set -euo pipefail'
      declare -f faucet bitcoin_cli send_to_address mine_blocks
      echo "$helper \"\$@\""
    } > "$bin/$helper"
    chmod +x "$bin/$helper"
  done
  export PATH="$bin:$PATH"
  export FM_SDK_INVITE_CODE="$invite" FM_SDK_WALLET_DIR="$wallet_a"
  cat <<BANNER

The federation is up, on shape ${FM_SDK_SHAPE:-v1}. In this shell:

  \$FM_SDK_INVITE_CODE   its invite code
  \$FM_SDK_WALLET_DIR    a fresh data directory (any other directory works too)

  run an example:   cargo run --example lightning -- \\
                      "\$FM_SDK_WALLET_DIR" "\$FM_SDK_INVITE_CODE" receive 100000
  pay an invoice:   faucet pay <invoice>
  get an invoice:   faucet invoice <msats>
  fund an address:  send_to_address <address> <sats>; mine_blocks 21
  bitcoin-cli:      bitcoin_cli <command ...>

Exit the shell to tear the federation down.

BANNER
  "${FM_SDK_SHELL:-bash}" || true
}

examples=("$@")
[ ${#examples[@]} -eq 0 ] && examples=(walkthrough lightning ecash onchain)

for name in "${examples[@]}"; do
  echo "running example: $name"
  case "$name" in
    walkthrough) run_walkthrough ;;
    lightning) run_lightning ;;
    ecash) run_ecash ;;
    onchain) run_onchain ;;
    shell) run_shell ;;
    *)
      echo "usage: run-sdk-examples-inner.sh [walkthrough|lightning|ecash|onchain|shell ...]" >&2
      exit 2
      ;;
  esac
done
