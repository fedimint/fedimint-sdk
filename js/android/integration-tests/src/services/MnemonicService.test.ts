/* eslint-disable no-console */
import {
  APP_TITLE,
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'

// The proving test for the harness itself: deliberately federation-free (pure
// local crypto, no devimint/faucet dependency) so it exercises the whole
// mechanical pipeline — build, install, launch, Appium interact, assert — with
// the fewest moving parts.
//
// What it covers on the SDK side is `createFedimintSdk` over the app-private
// directory and `exportMnemonic().words()`: the seed is generated and
// persisted by the native library, and read back through the generated Kotlin
// bindings. Named for the SDK capability it tests, not the screen it taps
// (mirroring js/web/integration-tests/src/services/*.test.ts).
export class MnemonicService extends AppiumTestBase {
  // Leaves an open SDK behind. Declaring it is what makes the runner reset
  // the app before any test that wants a fresh install (this one included, if
  // something ran before it) rather than inheriting whatever the last test
  // left on screen.
  static produces: readonly string[] = ['walletOpen']

  async execute(): Promise<void> {
    console.log('Starting MnemonicService test')

    await this.waitForText(APP_TITLE, 0, true, 30000)

    // Before anything is open the example app reports so, and "Show seed" is
    // disabled — asserting the starting point keeps a stale app left behind
    // by an earlier run from passing this test by accident.
    const initialStatus = await this.getTextByKey('walletStatus')
    if (!initialStatus.includes('SDK not open')) {
      throw new Error(
        `Expected a closed SDK on a fresh launch, got: "${initialStatus}"`,
      )
    }

    // Opening the SDK writes to the app-private directory and, over an empty
    // one, generates the seed. Given a cold emulator and a first-run rocksdb
    // open, this is slower than a UI tap — hence the longer budget.
    await this.clickElementByKey('openWallet')
    const opened = await this.waitForTextInElement(
      'walletResult',
      'opened',
      NETWORK_TIMEOUT,
    )
    console.log(`Wallet opened: ${opened}`)

    // `exportMnemonic()` hands back an opaque handle; the example app's "Show seed"
    // is the deliberate `words()` step that takes the phrase out as strings.
    await this.clickElementByKey('toggleSeed')
    const seed = await this.getTextByKey('seed')

    // The example app renders the phrase as a numbered grid ("1. abandon  2. …"),
    // so the indices come out with the words and are dropped here.
    const words = seed
      .trim()
      .split(/\s+/)
      .filter((token) => token.length > 0 && !/^\d+\.$/.test(token))

    if (words.length !== 12 && words.length !== 24) {
      throw new Error(
        `Expected a 12 or 24 word mnemonic, got ${words.length} words: "${seed}"`,
      )
    }
    if (!words.every((word) => /^[a-z]+$/.test(word))) {
      throw new Error(`Mnemonic contains a non-BIP39-looking word: "${seed}"`)
    }

    console.log(
      `MnemonicService test passed with a ${words.length}-word mnemonic`,
    )
  }
}
