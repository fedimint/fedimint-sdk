import { NETWORK_TIMEOUT } from '../env/constants'

import { Fixture } from './types'

/** Opens the SDK over the harness's storage, generating a seed on a fresh
 * install: the state every other fixture builds on. */
export const openWallet: Fixture = {
  produces: 'walletOpen',
  requires: [],
  methods: ['sdk.open'],

  async run({ h, log }) {
    // A first open builds the storage and generates the seed, which on a
    // cold device is slower than an ordinary call.
    const opened = await h.call('sdk.open', {}, { timeoutMs: NETWORK_TIMEOUT })
    log(`[fixture] opened, ${opened.federationIds.length} federation(s) stored`)
  },
}
