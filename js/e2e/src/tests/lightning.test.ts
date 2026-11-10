import { FUNDING_MSATS } from '../fixtures/fundWallet'
import { receiveOverLightning, waitForBalance } from '../flows/wallet'

import { TestDef } from './types'

/**
 * Lightning in, over a real federation and gateway: the harness issues an
 * invoice, devimint pays it, the receive is claimed, and the balance the
 * `balanceUpdates()` stream delivers goes up. Leaves the wallet funded for the
 * tests after it.
 */
export const lightningTest: TestDef = {
  name: 'lightning',
  prerequisites: ['walletOpen', 'joinedFederation'],
  produces: ['walletOpen', 'joinedFederation', 'funded'],
  requires: [
    'lightning.receive',
    'operation.observed',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, log } = ctx
    const updatesBefore = (await h.call('federation.observedBalance', {}))
      .updates

    const balance = await receiveOverLightning(ctx, FUNDING_MSATS)

    // The gateway's fee comes out of what arrives, and how much is the Rust
    // integration tests' business. A device proves the money arrived and the
    // live subscription delivered it, so it is bounded rather than pinned.
    if (balance <= 0 || balance > FUNDING_MSATS) {
      throw new Error(
        `Received ${balance} msat for a ${FUNDING_MSATS} msat invoice`,
      )
    }
    const { updates } = await h.call('federation.observedBalance', {})
    if (updates <= updatesBefore) {
      throw new Error('balanceUpdates() delivered nothing for the receive')
    }
    await waitForBalance(ctx, balance)
    log(`Received ${balance} msat`)
  },
}
