import { describe, expect, it } from 'vitest'
import { ErrorCode, LnReceiveState_Tags, openSdk } from '../index'

const faucet = import.meta.env.FAUCET

async function inviteCode(): Promise<string> {
  const res = await fetch(`${faucet}/connect-string`)
  if (!res.ok)
    throw new Error(
      `no invite code from the faucet (${res.status}): ${await res.text()}`,
    )
  return res.text()
}

async function payInvoice(invoice: string): Promise<void> {
  const res = await fetch(`${faucet}/pay`, { method: 'POST', body: invoice })
  if (!res.ok)
    throw new Error(
      `the faucet refused the invoice (${res.status}): ${await res.text()}`,
    )
}

const storage = `sdk-web-test-${Date.now()}`

describe('@fedimint/sdk-web against devimint', () => {
  it('joins, receives over lightning, and comes back after a restart', async () => {
    const session = await openSdk({ storage })
    try {
      expect(await session.sdk.federations()).toEqual([])

      await expect(
        session.InviteCode.parse('not-an-invite'),
      ).rejects.toMatchObject({
        code: ErrorCode.InvalidInput,
      })

      const invite = await session.InviteCode.parse(await inviteCode())
      const preview = await session.sdk.preview(invite)
      expect(preview.guardians).toBeGreaterThan(0)

      const federation = await session.sdk.join(invite)
      expect(await federation.balance()).toBe(0n)

      const lightning = await federation.lightning()
      expect(lightning).toBeDefined()
      const receive = await lightning!.receive(100_000n, 'sdk-web test')
      const updates = await receive.operation.updates()
      await payInvoice(receive.invoice)
      let state = await receive.operation.state()
      while (state.tag !== LnReceiveState_Tags.Claimed) {
        expect([
          LnReceiveState_Tags.Created,
          LnReceiveState_Tags.WaitingForPayment,
          LnReceiveState_Tags.Funded,
        ]).toContain(state.tag)
        const next = await updates.next()
        if (next === undefined) break
        state = next
      }
      expect(state.tag).toBe(LnReceiveState_Tags.Claimed)
      expect(await federation.balance()).toBeGreaterThan(0n)
    } finally {
      await session.close()
    }

    const again = await openSdk({ storage })
    try {
      const federations = await again.sdk.federations()
      expect(federations).toHaveLength(1)
      expect(await federations[0].balance()).toBeGreaterThan(0n)
    } finally {
      await again.close()
    }
  })

  it('a closed session rejects a call, and closing twice is a no-op', async () => {
    const session = await openSdk({ storage: `${storage}-closed` })
    const sdk = session.sdk
    await session.close()
    await expect(sdk.federations()).rejects.toMatchObject({
      name: 'SessionClosed',
    })
    await expect(session.close()).resolves.toBeUndefined()
  })
})
