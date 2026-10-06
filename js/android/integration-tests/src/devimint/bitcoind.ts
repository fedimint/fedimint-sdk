import { runCli } from './env'

// devimint's regtest bitcoind, through the `bitcoin-cli` command line it
// exports as FM_BTC_CLIENT. The faucet has no on-chain endpoint, so this is
// the only way to fund a deposit address or mine. A port of the helpers in
// rust/fedimint-sdk/tests/integration.rs (`bitcoin_cli`, `mine_blocks`,
// `send_to_address`).
//
// The wallet is named explicitly: devimint's bitcoind has several loaded (the
// gateways' among them), and `bitcoin-cli` refuses a wallet call without a
// selection then. `default` is the one devimint creates and funds for itself.

export function bitcoinCli(args: readonly string[]): string {
  return runCli('FM_BTC_CLIENT', ['-rpcwallet=default', ...args])
}

export function getNewAddress(): string {
  return bitcoinCli(['getnewaddress'])
}

/** Mines `n` blocks to a fresh address, as devimint's own peg-in helpers do. */
export function mineBlocks(n: number): void {
  bitcoinCli(['generatetoaddress', String(n), getNewAddress()])
}

/** Formats satoshis as the BTC decimal string bitcoind reads and prints. */
export function satsToBtc(sats: number): string {
  const whole = Math.floor(sats / 100_000_000)
  const frac = String(sats % 100_000_000).padStart(8, '0')
  return `${whole}.${frac}`
}

/** Sends `sats` to `address` from bitcoind's own wallet; returns the txid. */
export function sendToAddress(address: string, sats: number): string {
  return bitcoinCli(['sendtoaddress', address, satsToBtc(sats)])
}

/** What `address` has received with at least `minconf` confirmations, in BTC. */
export function receivedByAddress(address: string, minconf: number): string {
  return bitcoinCli(['getreceivedbyaddress', address, String(minconf)])
}
