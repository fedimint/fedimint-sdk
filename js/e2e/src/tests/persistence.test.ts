import { NETWORK_TIMEOUT } from '../env/constants'
import { waitForBalance } from '../flows/wallet'

import { TestDef } from './types'

/**
 * The storage outliving the process: kill the harness without clearing its
 * data, open the SDK over the same directory, and get back the same seed, the
 * same federation (reattached by the open) and the same balance. Opening
 * again also re-takes the storage lock the killed process held, which a
 * leaked lock would refuse with STORAGE_IN_USE.
 */
export const persistenceTest: TestDef = {
  name: 'persistence',
  prerequisites: ['walletOpen', 'joinedFederation', 'funded'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'sdk.mnemonic',
    'sdk.open',
    'federation.info',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, target, log } = ctx
    const { words } = await h.call('sdk.mnemonic', {})
    const { id } = await h.call('federation.info', {})
    const { amountMsats } = await h.call('federation.balance', {})
    if (amountMsats <= 0) {
      throw new Error(
        `Expected a funded wallet to persist, it holds ${amountMsats} msat`,
      )
    }

    await target.restart()
    const opened = await h.call('sdk.open', {}, { timeoutMs: NETWORK_TIMEOUT })
    if (
      !opened.federationIds.includes(id) ||
      opened.attachedFederationId !== id
    ) {
      throw new Error(
        `Reopening should reattach ${id}: ${JSON.stringify(opened)}`,
      )
    }

    const after = await h.call('sdk.mnemonic', {})
    if (after.words.join(' ') !== words.join(' ')) {
      throw new Error('The seed changed across a restart')
    }
    if ((await h.call('federation.info', {})).id !== id) {
      throw new Error('A different federation is attached after the restart')
    }
    await waitForBalance(ctx, amountMsats)
    log(`Seed, federation ${id} and ${amountMsats} msat survived a restart`)
  },
}
