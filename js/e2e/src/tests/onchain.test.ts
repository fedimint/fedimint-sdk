import {
  getNewAddress,
  mineBlocks,
  receivedByAddress,
  satsToBtc,
  sendToAddress,
} from '../env/bitcoind'
import { NETWORK_TIMEOUT } from '../env/constants'
import { sleep } from '../env/devimint'
import { waitForBalance, waitForState } from '../flows/wallet'

import { TestDef } from './types'

const DEPOSIT_SATS = 100_000
const WITHDRAW_SATS = 30_000
/** A deposit needs blocks mined and the federation to agree on them. */
const ONCHAIN_TIMEOUT = 180_000

/**
 * Bitcoin in and out through the wallet module: fund a deposit address from
 * devimint's bitcoind, mine it in, watch the claim credit exactly its net,
 * then withdraw to a bitcoind address, find the coins there, and see the
 * balance drop by exactly the quoted total. The Rust
 * `onchain_deposit_and_withdrawal_round_trip`, through the bindings.
 */
export const onchainTest: TestDef = {
  name: 'onchain',
  // `funded` is tolerated, not needed: the deposit funds this test.
  prerequisites: ['walletOpen', 'joinedFederation', 'funded'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'onchain.receive',
    'onchain.deposits',
    'onchain.quote',
    'onchain.send',
    'operation.observed',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, log } = ctx

    // ── Deposit ──────────────────────────────────────────────────────────
    const before = (await h.call('federation.balance', {})).amountMsats
    const { address } = await h.call(
      'onchain.receive',
      {},
      { timeoutMs: NETWORK_TIMEOUT },
    )
    if (!/^bcrt1[0-9a-z]+$/.test(address)) {
      throw new Error(`Not a regtest deposit address: ${address}`)
    }
    const txid = sendToAddress(address, DEPOSIT_SATS)
    log(`Sent ${DEPOSIT_SATS} sat to ${address} in ${txid}`)

    // The wallet finds the deposit once it has the confirmations the
    // federation requires; only then is there an operation.
    mineBlocks(21)
    const depositId = await h.poll(
      'onchain.deposits',
      {},
      (d) => {
        if (d.error) {
          throw new Error(
            `deposits() failed: [${d.error.code}] ${d.error.reason}`,
          )
        }
        return d.operationIds[0]
      },
      {
        timeoutMs: ONCHAIN_TIMEOUT,
        intervalMs: 2000,
        what: 'the deposit to be found',
      },
    )
    const claim = await waitForState(
      ctx,
      'ONCHAIN_RECEIVE',
      depositId,
      ['CLAIMED'],
      ONCHAIN_TIMEOUT,
    )
    if (claim.txid !== txid) {
      throw new Error(`Claimed ${claim.txid}, deposited in ${txid}`)
    }
    if (claim.grossDepositedSats !== DEPOSIT_SATS) {
      throw new Error(
        `Claimed a gross ${claim.grossDepositedSats} sat of a ${DEPOSIT_SATS} sat deposit`,
      )
    }
    const net = claim.netCreditMsats
    if (net <= 0 || net > DEPOSIT_SATS * 1000) {
      throw new Error(`Net credit ${net} msat for ${DEPOSIT_SATS} sat`)
    }
    // `netCredit` is defined as what the balance moved by.
    const funded = before + net
    await waitForBalance(ctx, funded, ONCHAIN_TIMEOUT)
    log(`Deposit claimed: +${net} msat`)

    // ── Withdrawal ───────────────────────────────────────────────────────
    const destination = getNewAddress()
    const quote = await h.call(
      'onchain.quote',
      { address: destination, amountSats: WITHDRAW_SATS },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    if (
      quote.amountSats !== WITHDRAW_SATS ||
      quote.totalMsats !== quote.amountSats * 1000 + quote.feeMsats
    ) {
      throw new Error(`Inconsistent withdrawal quote: ${JSON.stringify(quote)}`)
    }
    log(`Withdrawal quoted: ${WITHDRAW_SATS} sat + ${quote.feeMsats} msat fee`)

    const { operationId } = await h.call(
      'onchain.send',
      { quoteId: quote.quoteId },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    const sent = await waitForState(
      ctx,
      'ONCHAIN_SEND',
      operationId,
      ['SUCCEEDED'],
      ONCHAIN_TIMEOUT,
    )
    log(`Withdrawal broadcast in ${sent.txid}`)

    // SUCCEEDED is the federation's broadcast; the transaction reaches
    // bitcoind's mempool a moment later, so poll unconfirmed, then confirm.
    const expected = satsToBtc(WITHDRAW_SATS)
    const deadline = Date.now() + NETWORK_TIMEOUT
    while (receivedByAddress(destination, 0) !== expected) {
      if (Date.now() > deadline) {
        throw new Error(
          `The withdrawal ${sent.txid} never reached ${destination}`,
        )
      }
      await sleep(1000)
    }
    mineBlocks(1)
    const confirmed = receivedByAddress(destination, 1)
    if (confirmed !== expected) {
      throw new Error(
        `${destination} holds ${confirmed} BTC confirmed, expected ${expected}`,
      )
    }

    // Exactly the quoted total, once the change is minted.
    await waitForBalance(ctx, funded - quote.totalMsats, ONCHAIN_TIMEOUT)
    log(`Balance ${funded} -> ${funded - quote.totalMsats} msat`)
  },
}
