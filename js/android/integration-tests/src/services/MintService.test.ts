/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import {
  lookupOperation,
  operationIdFrom,
  parseMsats,
  readBalanceMsats,
  readSdkError,
  stateFrom,
  waitForBalance,
  waitForBalanceAbove,
} from '../flows/wallet'
import { currentShape, Shape } from '../shape'

/**
 * Amounts to try spending, in order, per generation.
 *
 * This SDK only hands out notes the wallet already holds in the exact
 * denominations needed — it never reissues itself change — and refuses a
 * quote it could not execute with NOT_SUPPORTED. Which amounts that allows
 * depends on how the funding receive happened to be split into notes, so the
 * test walks down a short list rather than pinning one. v1 notes are powers of
 * two msat; mintv2 rounds every amount up to a multiple of 512 msat, so its
 * list holds only such multiples, and a request is never silently rounded
 * past what the test expects.
 */
const SPEND_CANDIDATES_MSATS: Record<Shape, readonly number[]> = {
  v1: [10_000, 8_192, 4_096, 1_024],
  v2: [10_240, 8_192, 4_096, 1_024, 512],
}

// Ecash out and back in: spend notes, then reissue the very same notes into
// the wallet that made them. Mirrors the wasm suite's MintService tests, and
// covers the one thing those cannot — that an opaque handle (`Notes`) survives
// the round trip through the generated Kotlin, out as a string a user could
// paste, and back in.
//
// Runs on whatever LightningService left behind, so the federation is joined
// and funded once for both tests rather than per test.
export class MintService extends AppiumTestBase {
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
    console.log('Starting MintService test')
    const shape = currentShape()

    const before = await readBalanceMsats(this)

    // ── Out ──────────────────────────────────────────────────────────────
    const { sent, spent } = await this.spendSomeAmount(shape, before)
    const notes = notesFrom(sent)
    const sendId = operationIdFrom(sent)

    // "notes <value> + fee <fee>": the quote the send executed, which is
    // exactly what the balance moves by.
    const terms = sent.split('\n')[0]
    const [notesPart, feePart] = terms.split('+')
    const notesValue = parseMsats(notesPart)
    const fee = parseMsats(feePart ?? '')
    if (notesValue === null || fee === null) {
      throw new Error(`Could not read the send's terms out of "${terms}"`)
    }
    if (notesValue < spent) {
      throw new Error(
        `Asked for ${spent} msat of notes, got notes worth ${notesValue} msat`,
      )
    }
    console.log(
      `Spent ${spent} msat as ${notesValue} msat of notes + ${fee} msat fee ` +
        `(${notes.length} chars, operation ${sendId})`,
    )

    // The balance line follows `balanceUpdates()`, which delivers a moment
    // after the send returns, so the exact debit is waited for.
    const afterSend = before - notesValue - fee
    await waitForBalance(this, afterSend)

    // ── And back in ──────────────────────────────────────────────────────
    // Reissuing your own notes is an ordinary receive as far as the mint is
    // concerned: the notes are bearer instruments, and nothing ties them to
    // the wallet that asked for them.
    await this.scrollToElement('ecashNotes')
    await this.typeIntoElementByKey('ecashNotes', notes)
    await this.dismissKeyboard()
    await this.clickElementByKey('ecashReceive')

    // `awaitFinal`'s terminal state: only `Done` redeemed anything, and the
    // example app reports a `Failed` or an error otherwise.
    const redeemed = await this.waitForResultInElement(
      'ecashReceiveResult',
      NETWORK_TIMEOUT,
    )
    if (!redeemed.startsWith('Redeemed')) {
      throw new Error(`The notes were not redeemed: "${redeemed}"`)
    }
    console.log(redeemed.split('\n')[0])

    // The credit is the notes' value less whatever this generation charges to
    // reissue them (nothing on v1 by default, a per-note fee on mintv2), so it
    // is bounded rather than pinned: something came back, and no more than
    // the send took out.
    const afterReceive = await waitForBalanceAbove(this, afterSend)
    if (afterReceive > before) {
      throw new Error(
        `Redeeming ${notesValue} msat of notes moved the balance ${afterSend} -> ` +
          `${afterReceive} msat (it was ${before} before the send)`,
      )
    }

    // ── The send's own record ────────────────────────────────────────────
    // Reattached by id, the way an app would after a restart. Which state it
    // shows now is generation-specific: v1's executor settles an out-of-band
    // send only when its reclaim timer fires (a day away), and mintv2's
    // reclaim is driven by observing the send past its deadline — so neither
    // has to say `REDEEMED` yet. What neither may say is `CANCELED`: that
    // would mean the notes came back to this wallet a second time.
    //
    // `EcashSendState` has no data-carrying variant, so UniFFI generates it as
    // a plain Kotlin enum and the example app prints `state.name` — the
    // SCREAMING_SNAKE constants, not the PascalCase names the Rust enum and
    // the sealed-class states (LnReceiveState, OnchainSendState, ...) use.
    const record = await lookupOperation(this, sendId)
    if (!record.includes('kind ECASH_SEND')) {
      throw new Error(
        `Expected an ECASH_SEND record for ${sendId}: "${record}"`,
      )
    }
    const state = stateFrom(record)
    if (!['CREATED', 'REDEEMED', 'CANCEL_REQUESTED'].includes(state)) {
      throw new Error(`The redeemed send ${sendId} reports "${state}"`)
    }
    console.log(`Send ${sendId} reports ${state}`)

    console.log('MintService test passed')
  }

  /**
   * Spends the first candidate amount the wallet's notes can cover exactly.
   * Only NOT_SUPPORTED (exact change not possible) moves on to the next; any
   * other outcome is a failure of its own.
   */
  private async spendSomeAmount(
    shape: Shape,
    balance: number,
  ): Promise<{ sent: string; spent: number }> {
    const refused: string[] = []
    for (const amount of SPEND_CANDIDATES_MSATS[shape]) {
      if (amount > balance) continue
      await this.scrollToElement('ecashSendAmount')
      await this.typeIntoElementByKey('ecashSendAmount', String(amount))
      await this.dismissKeyboard()
      await this.clickElementByKey('ecashSend')

      const sent = await this.waitForResultInElement(
        'ecashSendResult',
        NETWORK_TIMEOUT,
      )
      if (sent.includes('hand these notes to the receiver')) {
        return { sent, spent: amount }
      }
      const error = readSdkError(sent)
      if (error?.code !== 'NOT_SUPPORTED') {
        throw new Error(`Sending ${amount} msat of ecash failed: "${sent}"`)
      }
      refused.push(`${amount}: ${error.reason}`)
      console.log(`${amount} msat needs change this wallet's notes cannot make`)
    }
    throw new Error(
      `No ${shape} ecash amount could be sent from a ${balance} msat balance:\n` +
        refused.join('\n'),
    )
  }
}

/**
 * Pulls the notes out of the send result.
 *
 * The example app prints them under a line of its own, because `Notes` is opaque and
 * `display()` is the deliberate way to take the token out — so the parse
 * follows that line rather than pattern-matching the token itself, which has
 * no fixed shape.
 */
function notesFrom(result: string): string {
  const lines = result.split('\n')
  const header = lines.findIndex((line) =>
    line.includes('hand these notes to the receiver'),
  )
  const notes = lines.slice(header + 1).find((line) => line.trim().length > 0)
  if (!notes) {
    throw new Error(`No notes under the header in: "${result}"`)
  }
  return notes.trim()
}
