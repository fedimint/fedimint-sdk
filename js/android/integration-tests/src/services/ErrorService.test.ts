/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import { sleep } from '../devimint/env'
import { FaucetClient } from '../faucet/FaucetClient'
import { expectSdkError, readBalanceMsats } from '../flows/wallet'

/** How long a refused payment gets to show a debit it should not make. */
const SETTLE_MS = 5_000

// Failures as an application sees them: every refusal here has to arrive as
// the SDK's structured `ErrorCode` across the FFI boundary — not a crash, not
// a bare message — and none of them may move money.
//
// Covers input the SDK can reject on its own (malformed notes, a malformed
// invoice, a zero amount) and a quote the federation-backed path refuses (an
// invoice larger than the balance), checking afterwards that the balance and
// the payment history are exactly as they were.
export class ErrorService extends AppiumTestBase {
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
    console.log('Starting ErrorService test')

    const before = await readBalanceMsats(this)
    const lnSendsBefore = await this.countActivity('LN_SEND')

    // ── Invalid input ────────────────────────────────────────────────────
    await this.scrollToElement('ecashNotes')
    await this.typeIntoElementByKey('ecashNotes', 'definitely-not-ecash-notes')
    await this.dismissKeyboard()
    await this.clickElementByKey('ecashReceive')
    expectSdkError(
      await this.waitForResultInElement('ecashReceiveResult'),
      'INVALID_INPUT',
    )
    console.log('Malformed notes: INVALID_INPUT')

    await this.scrollToElement('ecashSendAmount')
    await this.typeIntoElementByKey('ecashSendAmount', '0')
    await this.dismissKeyboard()
    await this.clickElementByKey('ecashSend')
    expectSdkError(
      await this.waitForResultInElement('ecashSendResult'),
      'INVALID_INPUT',
    )
    console.log('Zero ecash amount: INVALID_INPUT')

    await this.quoteInvoice('lnbc-not-an-invoice')
    expectSdkError(
      await this.waitForResultInElement('lnPayResult'),
      'INVALID_INPUT',
    )
    console.log('Malformed invoice: INVALID_INPUT')

    // ── A payment the balance cannot cover ───────────────────────────────
    // A real invoice from a reachable payee, so the only thing wrong with it
    // is its size: the quote has to refuse it up front rather than leave a
    // payment to fail later.
    const unpayable = before * 10 + 1_000_000
    const invoice = (await new FaucetClient().createInvoice(unpayable)).trim()
    await this.quoteInvoice(invoice)
    expectSdkError(
      await this.waitForResultInElement('lnPayResult', NETWORK_TIMEOUT),
      'INSUFFICIENT_BALANCE',
    )
    console.log(`Invoice for ${unpayable} msat: INSUFFICIENT_BALANCE`)

    // No quote was produced, so there is nothing to pay with: the example app
    // only enables Pay once it holds one.
    if (await this.isEnabledByKey('lnPay')) {
      throw new Error('Pay is enabled after a refused quote')
    }

    // ── Nothing moved ────────────────────────────────────────────────────
    await sleep(SETTLE_MS)
    const after = await readBalanceMsats(this)
    if (after !== before) {
      throw new Error(
        `Refused operations moved the balance: ${before} -> ${after} msat`,
      )
    }
    const lnSendsAfter = await this.countActivity('LN_SEND')
    if (lnSendsAfter !== lnSendsBefore) {
      throw new Error(
        `A refused payment left a lightning send in the history: ` +
          `${lnSendsBefore} -> ${lnSendsAfter} rows`,
      )
    }

    console.log('ErrorService test passed')
  }

  private async quoteInvoice(invoice: string): Promise<void> {
    await this.scrollToElement('lnInvoice')
    await this.typeIntoElementByKey('lnInvoice', invoice)
    await this.dismissKeyboard()
    await this.clickElementByKey('lnQuote')
  }

  /** How many rows of the recent activity are of `kind` (an OperationKind). */
  private async countActivity(kind: string): Promise<number> {
    await this.clickElementByKey('activity')
    const rows = await this.waitForResultInElement(
      'activityResult',
      NETWORK_TIMEOUT,
    )
    if (rows.includes('[')) {
      throw new Error(`Loading the activity failed: "${rows}"`)
    }
    return rows.split('\n').filter((line) => line.startsWith(`${kind} `)).length
  }
}
