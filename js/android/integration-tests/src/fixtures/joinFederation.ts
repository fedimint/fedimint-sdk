/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import { FaucetClient } from '../faucet/FaucetClient'

import { Fixture } from './types'

/**
 * Joins the local devimint federation, the way the wasm suite's `wallet`
 * fixture does: ask the faucet for an invite code, then join with it.
 *
 * The invite code names the guardians as `ws://127.0.0.1:<port>` on the
 * *host*. scripts/e2e-android/run-android-e2e.sh has already pointed those
 * ports back out of the emulator with `adb reverse`, so the app joins the
 * real federation through the code exactly as a user's app would, with no
 * test-only awareness of where it lives.
 *
 * Reachable only under devimint: `scripts/setup_test_shell.sh` execs the run
 * inside `devimint wasm-test-setup`, which is what puts FAUCET in reach.
 */
export const joinFederation: Fixture = {
  produces: 'joinedFederation',
  requires: ['walletOpen'],

  async run(t: AppiumTestBase): Promise<void> {
    const faucet = new FaucetClient()
    const inviteCode = (await faucet.getInviteCode()).trim()
    console.log(`[fixture] joining ${inviteCode.slice(0, 24)}…`)

    await t.scrollToElement('invite')
    await t.typeIntoElementByKey('invite', inviteCode)
    await t.dismissKeyboard()
    await t.clickElementByKey('join')

    // The example app reports both a fresh join and a reattach to one this storage
    // already holds; either leaves the federation current, which is all any
    // test downstream needs.
    const result = await t.waitForTextInElement(
      'joinResult',
      'oined',
      NETWORK_TIMEOUT,
    )
    console.log(`[fixture] ${result.split('\n')[0]}`)
  },
}
