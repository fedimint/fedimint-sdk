import { NETWORK_TIMEOUT } from '../env/constants'
import { sleep } from '../env/devimint'
import { paymentHash, sha256Hex, waitForLdkInvoicePaid } from '../env/lightning'
import { waitForBalance } from '../flows/wallet'

import { TestDef } from './types'

const INVOICE_MSATS = 20_000

/**
 * Lightning out to a node outside the federation, quote then send, checked on
 * both ends: the SDK's final state and exact debit here, the payee's own
 * record of the invoice there. Then the quote, which is single use, is sent
 * again and refused with QUOTE_EXPIRED, moving nothing.
 *
 * The payee is the faucet's LDK node; both generations route through
 * devimint's LND gateway (v1 picks the cheapest, and LND's fees are zero; on
 * v2 scripts/e2e/lib.sh pins the lnv2 gateway list to it).
 */
export const lightningSendTest: TestDef = {
  name: 'lightningSend',
  prerequisites: ['walletOpen', 'joinedFederation', 'funded'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'lightning.quote',
    'lightning.send',
    'operation.awaitFinal',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, faucet, log } = ctx
    const invoice = (await faucet.createInvoice(INVOICE_MSATS)).trim()
    const hash = paymentHash(invoice)
    const before = (await h.call('federation.balance', {})).amountMsats

    // ── Quote ────────────────────────────────────────────────────────────
    const quote = await h.call(
      'lightning.quote',
      { invoice },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    if (quote.invoiceAmountMsats !== INVOICE_MSATS) {
      throw new Error(
        `Quoted ${quote.invoiceAmountMsats} msat for a ${INVOICE_MSATS} msat invoice`,
      )
    }
    if (quote.totalMsats !== quote.invoiceAmountMsats + quote.feeMsats) {
      throw new Error(`Inconsistent lightning quote: ${JSON.stringify(quote)}`)
    }
    if (quote.route.type !== 'GATEWAY') {
      throw new Error(
        `An outside payee should be routed through a gateway: ${JSON.stringify(quote.route)}`,
      )
    }
    if (quote.totalMsats > before) {
      throw new Error(
        `The wallet holds ${before} msat, the payment needs ${quote.totalMsats}`,
      )
    }
    log(
      `Quoted ${quote.invoiceAmountMsats} + ${quote.feeMsats} = ${quote.totalMsats} msat`,
    )

    // ── Send ─────────────────────────────────────────────────────────────
    const { operationId } = await h.call(
      'lightning.send',
      { quoteId: quote.quoteId },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    const { state } = await h.call(
      'operation.awaitFinal',
      { operationId, timeoutMs: NETWORK_TIMEOUT },
      { timeoutMs: NETWORK_TIMEOUT + 10_000 },
    )
    if (state.type !== 'SUCCESS' || !('preimage' in state)) {
      throw new Error(`The payment did not succeed: ${JSON.stringify(state)}`)
    }
    // The preimage is the payee's to reveal: hashing to the invoice's
    // payment hash proves it was paid, not merely that the SDK thinks so.
    if (sha256Hex(state.preimage) !== hash) {
      throw new Error(`The preimage ${state.preimage} does not hash to ${hash}`)
    }

    // ── The other end, and the debit ─────────────────────────────────────
    await waitForLdkInvoicePaid(hash, NETWORK_TIMEOUT)
    const after = before - quote.totalMsats
    await waitForBalance(ctx, after)
    log(`The payee reports ${hash} paid; balance ${before} -> ${after} msat`)

    // ── The same quote again ─────────────────────────────────────────────
    const again = await h.try(
      'lightning.send',
      { quoteId: quote.quoteId },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    if (again.ok || again.error.code !== 'QUOTE_EXPIRED') {
      throw new Error(
        `Sending a spent quote again should answer QUOTE_EXPIRED: ${JSON.stringify(again)}`,
      )
    }
    await sleep(3_000)
    await waitForBalance(ctx, after)
  },
}
