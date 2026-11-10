import { currentShape, EXPECTED_MODULES, Shape } from '../env/shape'
import { NETWORK_TIMEOUT } from '../env/constants'
import { waitForBalance } from '../flows/wallet'

import { TestDef } from './types'

/**
 * What the SDK reports about the federation it joined (the join itself is the
 * `joinedFederation` fixture): regtest, all three capabilities, the module
 * kinds of this run's shape and none of the other's, one id however it is
 * read, and an empty balance.
 */
export const federationTest: TestDef = {
  name: 'federation',
  prerequisites: ['walletOpen', 'joinedFederation'],
  produces: ['walletOpen', 'joinedFederation'],
  requires: [
    'federation.info',
    'sdk.preview',
    'invite.parse',
    'federation.balance',
    'federation.observedBalance',
  ],

  async run(ctx) {
    const { h, faucet, log } = ctx
    const info = await h.call('federation.info', {})
    log(`Federation ${info.name ?? '(unnamed)'} ${info.id} on ${info.network}`)
    if (info.network !== 'REGTEST') {
      throw new Error(`Expected a regtest federation, got ${info.network}`)
    }
    // scripts/setup_test_shell.sh asks for a mint, a wallet and a lightning
    // module of one generation. A false here means the SDK read the config
    // wrong, which only a real federation catches.
    const { ecash, lightning, onchain } = info.capabilities
    if (!ecash || !lightning || !onchain) {
      throw new Error(
        `Expected every capability, got ${JSON.stringify(info.capabilities)}`,
      )
    }

    // Capabilities read the same on v1 and v2; the module kinds `preview`
    // lists tell the generations apart. This shape's must all be there, and
    // none of the other's.
    const inviteCode = (await faucet.getInviteCode()).trim()
    const preview = await h.call(
      'sdk.preview',
      { inviteCode },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    const shape = currentShape()
    const other: Shape = shape === 'v1' ? 'v2' : 'v1'
    const missing = EXPECTED_MODULES[shape].filter(
      (k) => !preview.modules.includes(k),
    )
    const foreign = EXPECTED_MODULES[other].filter((k) =>
      preview.modules.includes(k),
    )
    if (missing.length > 0 || foreign.length > 0) {
      throw new Error(
        `A ${shape} federation should run ${EXPECTED_MODULES[shape].join(', ')} and none of ` +
          `${EXPECTED_MODULES[other].join(', ')}; it runs: ${preview.modules.join(', ')}`,
      )
    }
    log(`Shape ${shape}, modules ${preview.modules.join(', ')}`)

    const { federationId } = await h.call('invite.parse', { inviteCode })
    if (preview.id !== info.id || federationId !== info.id) {
      throw new Error(
        `One federation, three ids: joined ${info.id}, previewed ${preview.id}, ` +
          `parsed ${federationId}`,
      )
    }

    // A freshly joined wallet holds nothing, by either path.
    await waitForBalance(ctx, 0)
  },
}
