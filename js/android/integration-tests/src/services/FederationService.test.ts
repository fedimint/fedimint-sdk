/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'
import { readBalanceMsats } from '../flows/wallet'

// The first test that needs a federation: the join itself happens in the
// `joinedFederation` fixture, and this asserts what the SDK reports about the
// federation it joined. The devimint counterpart of the wasm suite's
// FederationService/BalanceService tests, which likewise assert the join
// succeeded and that a fresh wallet holds nothing.
//
// Only reachable under devimint — see the fixture.
export class FederationService extends AppiumTestBase {
  static prerequisites: readonly string[] = ['walletOpen', 'joinedFederation']
  static produces: readonly string[] = ['walletOpen', 'joinedFederation']

  async execute(): Promise<void> {
    console.log('Starting FederationService test')

    // `refreshState()` rewrites this line from the live federation handle:
    // name, network, the three module capabilities and the id.
    const status = await this.getTextByKey('walletStatus')
    console.log(`Federation status: ${status.replace(/\n/g, ' | ')}`)

    if (!status.includes('Federation')) {
      throw new Error(`Expected a joined federation, got: "${status}"`)
    }
    // devimint's federation is regtest, and scripts/setup_test_shell.sh asks
    // for all three v1 modules, so all three capabilities must be true. A
    // false here means the SDK read the federation's config wrong, which is
    // exactly the kind of thing only a real federation catches.
    for (const capability of ['ecash true', 'lightning true', 'onchain true']) {
      if (!status.includes(capability)) {
        throw new Error(
          `Expected "${capability}" in the status line, got: "${status}"`,
        )
      }
    }
    if (!/\bregtest\b/i.test(status)) {
      throw new Error(`Expected a regtest federation, got: "${status}"`)
    }

    // A freshly joined wallet holds nothing, the same thing the wasm suite's
    // BalanceService asserts first.
    const balance = await readBalanceMsats(this)
    if (balance !== 0) {
      throw new Error(`Expected an empty wallet, got ${balance} msat`)
    }

    console.log('FederationService test passed')
  }
}
