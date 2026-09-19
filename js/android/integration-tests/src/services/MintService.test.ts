/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import { readBalanceMsats } from '../flows/wallet'

const SPEND_MSATS = 10_000

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

    const before = await readBalanceMsats(this)
    if (before < SPEND_MSATS) {
      throw new Error(
        `Need at least ${SPEND_MSATS} msat to spend, wallet holds ${before}`,
      )
    }

    // ── Out ──────────────────────────────────────────────────────────────
    await this.scrollToElement('ecashSendAmount')
    await this.typeIntoElementByKey('ecashSendAmount', String(SPEND_MSATS))
    await this.dismissKeyboard()
    await this.clickElementByKey('ecashSend')

    const sent = await this.waitForTextInElement(
      'ecashSendResult',
      'hand these notes to the receiver',
      NETWORK_TIMEOUT,
    )
    const notes = notesFrom(sent)
    console.log(`Spent ${SPEND_MSATS} msat as ${notes.length} chars of notes`)

    const afterSend = await readBalanceMsats(this)
    if (afterSend >= before) {
      throw new Error(
        `Balance did not drop after spending: ${before} -> ${afterSend} msat`,
      )
    }

    // ── And back in ──────────────────────────────────────────────────────
    // Reissuing your own notes is an ordinary receive as far as the mint is
    // concerned: the notes are bearer instruments, and nothing ties them to
    // the wallet that asked for them.
    await this.scrollToElement('ecashNotes')
    await this.typeIntoElementByKey('ecashNotes', notes)
    await this.dismissKeyboard()
    await this.clickElementByKey('ecashReceive')

    const redeemed = await this.waitForTextInElement(
      'ecashReceiveResult',
      'Redeemed',
      NETWORK_TIMEOUT,
    )
    console.log(redeemed.split('\n')[0])

    const afterReceive = await readBalanceMsats(this)
    if (afterReceive <= afterSend) {
      throw new Error(
        `Balance did not rise after redeeming: ${afterSend} -> ${afterReceive} msat`,
      )
    }

    console.log('MintService test passed')
  }
}

/**
 * Pulls the notes out of the send result.
 *
 * The demo prints them under a line of its own, because `Notes` is opaque and
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
