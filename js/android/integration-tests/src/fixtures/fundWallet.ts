/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'
import { receiveOverLightning } from '../flows/wallet'

import { Fixture } from './types'

/** What the faucet pays in, in millisatoshis. The wasm suite's `fundedWallet`
 * uses 10_000; this asks for what the Rust lightning-send test funds with, so
 * the tests that inherit it — an ecash round trip, a lightning send, the
 * error checks — fit after v2's per-transaction fees without each arranging
 * its own funding. */
export const FUNDING_MSATS = 200_000

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
