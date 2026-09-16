#!/usr/bin/env bash
#
# devimint's --exec target for scripts/run-sdk-examples.sh; do not run this directly. It assumes
# the environment devimint's wasm-test-setup exports (FM_SDK_SHAPE, FM_MINT_CLIENT,
# FM_PORT_GW_LND and the rest) and that the examples are already built.
#
#   scripts/run-sdk-examples-inner.sh [example ...]
#
# Defaults to running all four examples: walkthrough, ecash, lightning, onchain.

set -euo pipefail

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
  # FM_MINT_CLIENT is a ready-to-run command line, so it is deliberately left unquoted below to
  # split it into a program and its arguments, the same way the tests' Rust harness does.
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

examples=("$@")
[ ${#examples[@]} -eq 0 ] && examples=(walkthrough ecash lightning onchain)

for name in "${examples[@]}"; do
  echo "running example: $name"
  if ! cargo run --locked --example "$name"; then
    echo "error: example '$name' failed" >&2
    exit 1
  fi
done
