/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'

// v1 proving test for the harness itself: deliberately federation-free (pure
// local crypto, no devimint/faucet dependency) so it exercises the full
// mechanical pipeline — build, install, launch, Appium interact, assert —
// with the fewest possible moving parts. Mirrors the naming convention of
// js/web/integration-tests/src/services/*.test.ts: this tests the SDK's
// mnemonic capability, not "the example app's UI".
export class MnemonicService extends AppiumTestBase {
  async execute(): Promise<void> {
    console.log('Starting MnemonicService test')

    await this.waitForText('Fedimint Typescript Library Demo', 0, true, 30000)

    // "Generate" is used by both the Mnemonic Manager and Deposit
    // sections, so this one button has an explicit testID
    // (see js/examples/react-native/src/App.tsx) rather than relying on
    // ambiguous text matching.
    await this.clickElementByKey('GenerateMnemonicButton')

    await this.waitForText('New mnemonic generated!', 0, false)

    // The generated mnemonic is a value field read back, and has no
    // stable, unique text to match on before it exists — a case where a
    // testID earns its keep (see the README's testID guidance).
    const mnemonic = await this.getTextByKey('MnemonicText')
    const words = mnemonic.trim().split(/\s+/)
    if (words.length !== 12 && words.length !== 24) {
      throw new Error(
        `Expected a 12 or 24 word mnemonic, got ${words.length} words: "${mnemonic}"`,
      )
    }

    console.log(
      `MnemonicService test passed with a ${words.length}-word mnemonic`,
    )
  }
}
