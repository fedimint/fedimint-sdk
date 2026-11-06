import { NETWORK_TIMEOUT } from '../env/constants'
import { sleep } from '../env/devimint'
import { countActivity, waitForBalance } from '../flows/wallet'
import { Method, ParamsOf } from '../protocol/methods'

import { TestContext, TestDef } from './types'

/** How long a refused payment gets to show a debit it should not make. */
const SETTLE_MS = 5_000

/**
 * Failures as an application sees them: every refusal arrives as the SDK's
 * structured ErrorCode across the binding, and none moves money. Input the
 * SDK rejects on its own (malformed notes and invoice, a zero amount), and a
 * quote the federation-backed path refuses (an invoice above the balance),
 * which must produce no quote to pay with.
 */
export const errorsTest: TestDef = {
  name: 'errors',
  prerequisites: ['walletOpen', 'joinedFederation', 'funded'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'ecash.receive',
    'ecash.quote',
    'lightning.quote',
    'activity.list',
    'harness.state',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, faucet, log } = ctx
    const before = (await h.call('federation.balance', {})).amountMsats
    const lnSendsBefore = await countActivity(ctx, 'LN_SEND')

    await expectError(
      ctx,
      'ecash.receive',
      { notes: 'definitely-not-ecash-notes' },
      'INVALID_INPUT',
    )
    await expectError(ctx, 'ecash.quote', { amountMsats: 0 }, 'INVALID_INPUT')
    await expectError(
      ctx,
      'lightning.quote',
      { invoice: 'lnbc-not-an-invoice' },
      'INVALID_INPUT',
    )

    // A real invoice from a reachable payee, wrong only in its size: the
    // quote has to refuse it up front, and hold no quote to pay with.
    const quotesBefore = (await h.call('harness.state', {})).quotes
    const unpayable = before * 10 + 1_000_000
    const invoice = (await faucet.createInvoice(unpayable)).trim()
    await expectError(
      ctx,
      'lightning.quote',
      { invoice },
      'INSUFFICIENT_BALANCE',
    )
    const quotesAfter = (await h.call('harness.state', {})).quotes
    if (quotesAfter !== quotesBefore) {
      throw new Error('A refused quote left a quote behind to pay with')
    }

    // Nothing moved.
    await sleep(SETTLE_MS)
    await waitForBalance(ctx, before)
    const lnSendsAfter = await countActivity(ctx, 'LN_SEND')
    if (lnSendsAfter !== lnSendsBefore) {
      throw new Error(
        `A refused payment left a lightning send in the history: ${lnSendsBefore} -> ${lnSendsAfter}`,
      )
    }
    log('Every refusal carried its ErrorCode and nothing moved')
  },
}

async function expectError<M extends Method>(
  { h, log }: TestContext,
  method: M,
  params: ParamsOf<M>,
  code: string,
): Promise<void> {
  const outcome = await h.try(method, params, { timeoutMs: NETWORK_TIMEOUT })
  if (outcome.ok || outcome.error.code !== code) {
    throw new Error(
      `${method} ${JSON.stringify(params).slice(0, 80)} should answer ${code}: ` +
        JSON.stringify(outcome),
    )
  }
  log(`${method}: ${code}`)
}
