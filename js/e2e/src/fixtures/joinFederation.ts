import { NETWORK_TIMEOUT } from '../env/constants'

import { Fixture } from './types'

/**
 * Joins the local devimint federation through the invite code its faucet
 * hands out. The code names the guardians on the host's 127.0.0.1; each
 * target's provisioning makes that reachable (adb reverse on Android), so the
 * harness joins through the unmodified code like any other client.
 */
export const joinFederation: Fixture = {
  produces: 'joinedFederation',
  requires: ['walletOpen'],
  methods: ['sdk.join', 'invite.parse', 'harness.state'],

  async run({ h, faucet, log }) {
    const inviteCode = (await faucet.getInviteCode()).trim()
    log(`[fixture] joining ${inviteCode.slice(0, 24)}…`)
    const joined = await h.try(
      'sdk.join',
      { inviteCode },
      { timeoutMs: NETWORK_TIMEOUT },
    )
    if (joined.ok) return
    // Already joined and reattached by `sdk.open` is just as good.
    if (joined.error.code === 'ALREADY_JOINED') {
      const { federationId } = await h.call('invite.parse', { inviteCode })
      const state = await h.call('harness.state', {})
      if (state.federationId === federationId) return
    }
    throw new Error(
      `Joining failed: [${joined.error.code}] ${joined.error.reason}`,
    )
  },
}
