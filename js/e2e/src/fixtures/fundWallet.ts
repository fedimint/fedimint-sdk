import { receiveOverLightning } from '../flows/wallet'

import { Fixture } from './types'

/** What the faucet pays in, in millisatoshis: enough for the tests that spend
 * (an ecash round trip, a lightning send, the error checks) to fit after v2's
 * per-transaction fees without each arranging its own funding. */
export const FUNDING_MSATS = 200_000

/** Leaves the wallet holding ecash, so a test that spends need not set it up. */
export const fundWallet: Fixture = {
  produces: 'funded',
  requires: ['walletOpen', 'joinedFederation'],
  methods: [
    'federation.balance',
    'federation.observedBalance',
    'lightning.receive',
    'operation.observed',
  ],

  async run(ctx) {
    ctx.log(`[fixture] funding with ${FUNDING_MSATS} msat`)
    await receiveOverLightning(ctx, FUNDING_MSATS)
  },
}
