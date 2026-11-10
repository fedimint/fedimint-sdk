import { NETWORK_TIMEOUT } from '../env/constants'

import { TestDef } from './types'

/**
 * `createFedimintSdk` over the harness's storage, and `exportMnemonic().words()`:
 * the seed is generated and persisted by the native library and read back
 * through the bindings. No federation.
 */
export const mnemonicTest: TestDef = {
  name: 'mnemonic',
  prerequisites: [],
  produces: ['walletOpen'],
  requires: ['harness.state', 'sdk.open', 'sdk.mnemonic'],

  async run({ h, log }) {
    // Assert the starting point, so an SDK left open by an earlier run cannot
    // pass this test by accident.
    if ((await h.call('harness.state', {})).sdkOpen) {
      throw new Error('Expected a closed SDK on a fresh install')
    }
    const early = await h.try('sdk.mnemonic', {})
    if (early.ok || early.error.code !== 'HARNESS_STATE') {
      throw new Error(
        `Reading the seed before opening should answer HARNESS_STATE: ${JSON.stringify(early)}`,
      )
    }

    const opened = await h.call('sdk.open', {}, { timeoutMs: NETWORK_TIMEOUT })
    if (
      opened.federationIds.length > 0 ||
      opened.attachedFederationId !== null
    ) {
      throw new Error(
        `A fresh install should hold no federation: ${JSON.stringify(opened)}`,
      )
    }

    const { words } = await h.call('sdk.mnemonic', {})
    if (words.length !== 12 && words.length !== 24) {
      throw new Error(`Expected 12 or 24 words, got ${words.length}`)
    }
    if (!words.every((w) => /^[a-z]+$/.test(w))) {
      throw new Error('The mnemonic holds a word that is not lowercase a-z')
    }
    log(`A ${words.length}-word mnemonic`)
  },
}
