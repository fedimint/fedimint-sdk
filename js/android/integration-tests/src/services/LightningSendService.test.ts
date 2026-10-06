/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import {
  paymentHash,
  sha256Hex,
  waitForLdkInvoicePaid,
} from '../devimint/lightning'
import { FaucetClient } from '../faucet/FaucetClient'
import {
  operationIdFrom,
  quoteLineMsats,
  readBalanceMsats,
  readSdkError,
  waitForBalance,
} from '../flows/wallet'
import { currentShape } from '../shape'

const INVOICE_MSATS = 20_000

// Lightning out: an invoice from a node outside the federation, quoted and
// paid through the app's quote → send path, with the outcome checked on both
// ends — the SDK's final state and debit on this side, and the payee's own
// record of the invoice on the other.
//
// The payee is the faucet's LDK node. Both generations route through
// devimint's LND gateway: on v1 the SDK picks the cheapest gateway and
// wasm-test-setup sets LND's fees to zero, and on v2 run-android-e2e.sh pins
// the lnv2 gateway list to it — so the payment always crosses from one node
// to the other rather than asking a node to pay itself.
export class LightningSendService extends AppiumTestBase {
  static prerequisites: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]
  static produces: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]

  async execute(): Promise<void> {
    console.log('Starting LightningSendService test')
    const shape = currentShape()

    const invoice = (
      await new FaucetClient().createInvoice(INVOICE_MSATS)
    ).trim()
    const hash = paymentHash(invoice)
    console.log(
      `Paying ${INVOICE_MSATS} msat to ${invoice.slice(0, 24)}… (${hash})`,
    )

    const before = await readBalanceMsats(this)

    // ── Quote ────────────────────────────────────────────────────────────
    await this.scrollToElement('lnInvoice')
    await this.typeIntoElementByKey('lnInvoice', invoice)
    await this.dismissKeyboard()
    await this.clickElementByKey('lnQuote')
    const quote = await this.waitForResultInElement(
      'lnPayResult',
      NETWORK_TIMEOUT,
    )
    if (!quote.includes('Total')) {
      throw new Error(`Quoting the invoice failed: "${quote}"`)
    }
    const amount = quoteLineMsats(quote, 'Amount')
    const fee = quoteLineMsats(quote, 'Fee')
    const total = quoteLineMsats(quote, 'Total')
    if (amount !== INVOICE_MSATS) {
      throw new Error(
        `Quoted ${amount} msat for a ${INVOICE_MSATS} msat invoice`,
      )
    }
    if (total !== amount + fee) {
      throw new Error(
        `Quote total ${total} is not amount ${amount} + fee ${fee}`,
      )
    }
    if (!quote.includes('Gateway(')) {
      throw new Error(
        `An outside payee should be routed through a gateway: "${quote}"`,
      )
    }
    if (total > before) {
      throw new Error(
        `The wallet holds ${before} msat, the payment needs ${total}`,
      )
    }
    console.log(`Quoted ${amount} + ${fee} fee = ${total} msat`)

    // ── Send ─────────────────────────────────────────────────────────────
    await this.clickElementByKey('lnPay')
    // `awaitFinal`'s terminal state, or the error that ended the wait.
    const outcome = await this.waitForResultInElement(
      'lnPayResult',
      NETWORK_TIMEOUT,
      (text) => text.includes('Total'),
    )
    console.log(`Send outcome: ${outcome.replace(/\n/g, ' | ')}`)

    const preimage = outcome.match(/Success\(.*preimage=([0-9a-f]{64})\)/)?.[1]
    if (preimage) {
      // The preimage is the payee's to reveal: hashing to the invoice's
      // payment hash proves the invoice was paid, not merely that the SDK
      // thinks so.
      if (sha256Hex(preimage) !== hash) {
        throw new Error(`The preimage ${preimage} does not hash to ${hash}`)
      }
      operationIdFrom(outcome)
    } else {
      const error = readSdkError(outcome)
      // fedimint/fedimint#8969: the v1 client strips two characters off the
      // preimage the gateway returns, so the SDK cannot decode the success
      // state. The payment itself goes through, which the payee check below
      // still requires; only this one, known, observation failure is let
      // through, and only on v1. Mirrors
      // rust/fedimint-sdk/tests/integration.rs's lightning send test.
      const knownV1Failure =
        shape === 'v1' &&
        error?.code === 'INTERNAL' &&
        error.reason.includes('preimage')
      if (!knownV1Failure) {
        throw new Error(`The payment did not succeed: "${outcome}"`)
      }
      console.log(
        'v1 send outcome undecodable (fedimint#8969); checking the payee',
      )
    }

    // ── The other end ────────────────────────────────────────────────────
    await waitForLdkInvoicePaid(hash, NETWORK_TIMEOUT)
    console.log(`The payee reports ${hash} as paid`)

    // Exactly the quoted total, once the change from the funding transaction
    // has been minted.
    await waitForBalance(this, before - total)
    console.log(`Balance ${before} -> ${before - total} msat`)

    console.log('LightningSendService test passed')
  }
}
