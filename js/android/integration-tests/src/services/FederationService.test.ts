/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import { readBalanceMsats } from '../flows/wallet'
import { currentShape, EXPECTED_MODULES, Shape } from '../shape'

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
    // for a mint, a wallet and a lightning module of one generation, so all
    // three capabilities must be true. A false here means the SDK read the
    // federation's config wrong, which is exactly the kind of thing only a
    // real federation catches.
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

    await this.assertModuleKinds(currentShape())

    // A freshly joined wallet holds nothing, the same thing the wasm suite's
    // BalanceService asserts first.
    const balance = await readBalanceMsats(this)
    if (balance !== 0) {
      throw new Error(`Expected an empty wallet, got ${balance} msat`)
    }

    console.log('FederationService test passed')
  }

  /**
   * The capabilities above read the same on v1 and v2, so they cannot say
   * which generation the SDK is driving. `preview` lists the kind of every
   * module the federation runs, which can: this run's shape has to be all
   * there, and none of the other generation's — the SDK would have refused
   * the join over a mix, but a federation of the wrong shape would not.
   */
  private async assertModuleKinds(shape: Shape): Promise<void> {
    // The join fixture left the federation's own invite code in the field.
    await this.clickElementByKey('preview')
    const preview = await this.waitForTextInElement(
      'joinResult',
      'Modules',
      NETWORK_TIMEOUT,
    )
    const line = preview.split('\n').find((l) => l.startsWith('Modules'))
    const kinds = (line ?? '')
      .replace('Modules', '')
      .split(',')
      .map((kind) => kind.trim())
      .filter((kind) => kind.length > 0)
    console.log(`Federation shape ${shape}, module kinds: ${kinds.join(', ')}`)

    const other: Shape = shape === 'v1' ? 'v2' : 'v1'
    const missing = EXPECTED_MODULES[shape].filter((k) => !kinds.includes(k))
    const foreign = EXPECTED_MODULES[other].filter((k) => kinds.includes(k))
    if (missing.length > 0 || foreign.length > 0) {
      throw new Error(
        `A ${shape} federation should run ${EXPECTED_MODULES[shape].join(', ')} and none of ` +
          `${EXPECTED_MODULES[other].join(', ')}; it runs: ${kinds.join(', ')}`,
      )
    }
  }
}
