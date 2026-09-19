/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'

import { Fixture } from './types'

/**
 * Opens the SDK over the app-private directory, generating and persisting a
 * seed if there isn't one — the state every other fixture builds on.
 *
 * The equivalent of the wasm suite's `wallet` fixture
 * (js/web/integration-tests/src/test/fixtures.ts) up to the point before it
 * joins: create the wallet, generate the mnemonic.
 */
export const openWallet: Fixture = {
  produces: 'walletOpen',
  requires: [],

  async run(t: AppiumTestBase): Promise<void> {
    console.log('[fixture] opening the wallet')

    await t.waitForText('Fedimint Android SDK Demo', 0, true, 30000)
    await t.clickElementByKey('openWallet')

    // A first open builds the storage and generates a seed, which on a cold
    // emulator is slower than any UI interaction here.
    const result = await t.waitForTextInElement(
      'walletResult',
      'opened',
      NETWORK_TIMEOUT,
    )
    console.log(`[fixture] ${result}`)
  },
}
