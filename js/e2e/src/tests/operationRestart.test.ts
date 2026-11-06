import { NETWORK_TIMEOUT } from '../env/constants'
import {
  payFromOutside,
  waitForBalanceAbove,
  waitForState,
} from '../flows/wallet'

import { TestDef } from './types'

const RECEIVE_MSATS = 30_000

/**
 * An operation outliving the process that started it: issue an invoice, kill
 * the harness before it is paid, reopen, find the receive again by id through
 * `federation.operation`, and follow the reattached handle to CLAIMED once
 * the invoice is paid. The persisted operation, not the app's memory, has to
 * finish the job.
 */
export const operationRestartTest: TestDef = {
  name: 'operationRestart',
  // `funded` is tolerated rather than needed, so a run of `all` continues on
  // the previous test's wallet instead of resetting.
  prerequisites: ['walletOpen', 'joinedFederation', 'funded'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'lightning.receive',
    'sdk.open',
    'operation.lookup',
    'operation.observed',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, target, log } = ctx
    const { invoice, operationId } = await h.call('lightning.receive', {
      amountMsats: RECEIVE_MSATS,
      description: 'e2e restart',
    })
    log(`Receive ${operationId} issued`)

    await target.restart()
    await h.call('sdk.open', {}, { timeoutMs: NETWORK_TIMEOUT })
    const before = (await h.call('federation.balance', {})).amountMsats

    const record = await h.call('operation.lookup', { operationId })
    if (!record.found || record.kind !== 'LN_RECEIVE' || !record.state) {
      throw new Error(
        `Expected an LN_RECEIVE record: ${JSON.stringify(record)}`,
      )
    }
    if (!['CREATED', 'WAITING_FOR_PAYMENT'].includes(record.state.type)) {
      throw new Error(
        `An unpaid receive should still be waiting after a restart, reports ${record.state.type}`,
      )
    }
    log(`Reattached to ${operationId} in ${record.state.type}`)

    await payFromOutside(invoice)
    // The lookup started a watcher on the reattached handle.
    await waitForState(ctx, 'LN_RECEIVE', operationId, ['CLAIMED'])
    const after = await waitForBalanceAbove(ctx, before)
    if (after - before > RECEIVE_MSATS) {
      throw new Error(
        `Received ${after - before} msat for a ${RECEIVE_MSATS} msat invoice`,
      )
    }
  },
}
