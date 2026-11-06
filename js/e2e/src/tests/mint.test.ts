import { NETWORK_TIMEOUT } from '../env/constants'
import { currentShape, Shape } from '../env/shape'
import { waitForBalance, waitForBalanceAbove } from '../flows/wallet'
import { ResultOf } from '../protocol/methods'

import { TestContext, TestDef } from './types'

/**
 * Amounts to try spending, in order, per generation. This SDK only hands out
 * notes the wallet holds in exact denominations and refuses one it cannot
 * make with NOT_SUPPORTED; which amounts work depends on how the funding was
 * split, so the test walks a short list. v1 notes are powers of two msat;
 * mintv2 rounds up to a multiple of 512 msat, so its list holds only those.
 */
const SPEND_CANDIDATES_MSATS: Record<Shape, readonly number[]> = {
  v1: [10_000, 8_192, 4_096, 1_024],
  v2: [10_240, 8_192, 4_096, 1_024, 512],
}

/**
 * Ecash out and back in: quote and send notes, then reissue the same notes
 * into the wallet that made them. The debit is exactly the quote, the
 * reissue reaches DONE, and the send's own record is ECASH_SEND and not
 * CANCELED. The notes leave as `display()` and come back through
 * `Notes.parse`, inside the harness.
 */
export const mintTest: TestDef = {
  name: 'mint',
  prerequisites: ['walletOpen', 'joinedFederation', 'funded'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'ecash.quote',
    'ecash.send',
    'ecash.receive',
    'operation.awaitFinal',
    'operation.lookup',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, log } = ctx
    const before = (await h.call('federation.balance', {})).amountMsats

    // ── Out ──────────────────────────────────────────────────────────────
    const { quote, sent, asked } = await spendSomeAmount(ctx, before)
    if (quote.notesValueMsats < asked) {
      throw new Error(
        `Asked for ${asked} msat of notes, the quote gives ${quote.notesValueMsats}`,
      )
    }
    if (quote.totalMsats !== quote.notesValueMsats + quote.feeMsats) {
      throw new Error(`Inconsistent ecash quote: ${JSON.stringify(quote)}`)
    }
    if (sent.notesValueMsats !== quote.notesValueMsats) {
      throw new Error(
        `Quoted ${quote.notesValueMsats} msat of notes, sent ${sent.notesValueMsats}`,
      )
    }
    log(
      `Sent ${quote.notesValueMsats} msat of notes + ${quote.feeMsats} fee ` +
        `(operation ${sent.operationId})`,
    )
    const afterSend = before - quote.totalMsats
    await waitForBalance(ctx, afterSend)

    // ── And back in ──────────────────────────────────────────────────────
    // Notes are bearer instruments: reissuing your own is an ordinary receive.
    const received = await h.call(
      'ecash.receive',
      { notesRef: sent.notesRef },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    if (received.notesValueMsats !== quote.notesValueMsats) {
      throw new Error(
        `Parsed notes worth ${received.notesValueMsats} msat, sent ${quote.notesValueMsats}`,
      )
    }
    const final = await h.call(
      'operation.awaitFinal',
      { operationId: received.operationId, timeoutMs: NETWORK_TIMEOUT },
      { timeoutMs: NETWORK_TIMEOUT + 10_000 },
    )
    if (final.state.type !== 'DONE') {
      throw new Error(`The notes were not redeemed: ${JSON.stringify(final)}`)
    }
    // The credit is the notes less any reissue fee (none on v1 by default,
    // per note on mintv2), so it is bounded: something came back, and no
    // more than the send took out.
    const afterReceive = await waitForBalanceAbove(ctx, afterSend)
    if (afterReceive > before) {
      throw new Error(
        `Redeeming moved the balance ${afterSend} -> ${afterReceive} msat ` +
          `(it was ${before} before the send)`,
      )
    }

    // ── The send's own record ────────────────────────────────────────────
    // Looked up by id, the way an app would after a restart. Neither
    // generation has to say REDEEMED yet (v1 settles an out-of-band send on a
    // day-long reclaim timer, mintv2 when it observes the deadline pass), but
    // CANCELED would mean the notes came back a second time.
    const record = await h.call('operation.lookup', {
      operationId: sent.operationId,
    })
    if (!record.found || record.kind !== 'ECASH_SEND' || !record.state) {
      throw new Error(
        `Expected an ECASH_SEND record: ${JSON.stringify(record)}`,
      )
    }
    if (
      !['CREATED', 'REDEEMED', 'CANCEL_REQUESTED'].includes(record.state.type)
    ) {
      throw new Error(`The redeemed send reports ${record.state.type}`)
    }
    log(`Send ${sent.operationId} reports ${record.state.type}`)
  },
}

/**
 * Quotes and sends the first candidate the wallet's notes cover exactly.
 * Only NOT_SUPPORTED (no exact change) moves on; any other refusal fails.
 */
async function spendSomeAmount(
  { h, log }: TestContext,
  balance: number,
): Promise<{
  quote: ResultOf<'ecash.quote'>
  sent: ResultOf<'ecash.send'>
  asked: number
}> {
  const shape = currentShape()
  const refused: string[] = []
  for (const amount of SPEND_CANDIDATES_MSATS[shape]) {
    if (amount > balance) continue
    const quote = await h.try('ecash.quote', { amountMsats: amount })
    let error = quote.ok ? null : quote.error
    if (quote.ok) {
      const sent = await h.try(
        'ecash.send',
        { quoteId: quote.result.quoteId },
        { timeoutMs: NETWORK_TIMEOUT },
      )
      if (sent.ok)
        return { quote: quote.result, sent: sent.result, asked: amount }
      error = sent.error
    }
    if (error?.code !== 'NOT_SUPPORTED') {
      throw new Error(
        `Sending ${amount} msat of ecash failed: ${JSON.stringify(error)}`,
      )
    }
    refused.push(`${amount}: ${error.reason}`)
    log(`${amount} msat needs change this wallet's notes cannot make`)
  }
  throw new Error(
    `No ${shape} ecash amount could be sent from ${balance} msat:\n${refused.join('\n')}`,
  )
}
