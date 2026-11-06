import { MUTINYNET_INVITE_CODE } from '../env/constants'

import { TestDef } from './types'

/**
 * `InviteCode.parse` and `federationId()`: a value type built from a string,
 * offline. It needs an open SDK only because the web SDK hands out its statics
 * from an open session, and the test is the same everywhere.
 */
export const inviteCodeTest: TestDef = {
  name: 'inviteCode',
  prerequisites: ['walletOpen'],
  produces: [],
  requires: ['invite.parse'],

  async run({ h, log }) {
    // The decoder already requires 32 bytes of hex: the hash of the
    // guardian set.
    const { federationId } = await h.call('invite.parse', {
      inviteCode: MUTINYNET_INVITE_CODE,
    })
    log(`Parsed federation ${federationId}`)

    const bad = await h.try('invite.parse', { inviteCode: 'fed1notacode' })
    if (bad.ok || bad.error.code !== 'INVALID_INPUT') {
      throw new Error(
        `A malformed invite code should answer INVALID_INPUT: ${JSON.stringify(bad)}`,
      )
    }
  },
}
