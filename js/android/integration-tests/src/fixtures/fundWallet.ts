/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'
import { receiveOverLightning } from '../flows/wallet'

import { Fixture } from './types'

/** What the faucet pays in, in millisatoshis. The wasm suite's `fundedWallet`
 * uses 10_000; this asks for more so a test can spend a few times without
 * arranging its own funding. */
export const FUNDING_MSATS = 50_000

/**
 * Leaves the wallet holding ecash, so a test that spends doesn't have to set
 * that up itself. The Android twin of the wasm suite's `fundedWallet`.
 */
export const fundWallet: Fixture = {
  produces: 'funded',
  requires: ['walletOpen', 'joinedFederation'],

  async run(t: AppiumTestBase): Promise<void> {
    console.log(`[fixture] funding with ${FUNDING_MSATS} msat`)
    await receiveOverLightning(t, FUNDING_MSATS)
  },
}
