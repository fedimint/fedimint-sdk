import { describe, expect } from 'vitest'
import {
  ActivityStatus,
  Direction,
  ErrorCode,
  FederationStatus_Tags,
  LnReceiveState_Tags,
  Network,
  OperationKind,
  OperationSupport,
} from '@fedimint/web-sdk'
import { fund, nextMatching, sdkTest } from './fixtures'

describe('preview and join', () => {
  sdkTest(
    "previews devimint's federation without joining it",
    async ({ session, invite }) => {
      const preview = await session.sdk.preview(invite)
      expect(preview.id).toMatch(/^[0-9a-f]{64}$/)
      expect(preview.id).toBe(await invite.federationId())
      expect(preview.network).toBe(Network.Regtest)
      expect(preview.guardians).toBeGreaterThan(0)
      // The v1 shape scripts/setup_test_shell.sh stands up.
      expect(preview.modules).toEqual(
        expect.arrayContaining(['mint', 'wallet', 'ln']),
      )
      expect(preview.meta).toBeInstanceOf(Map)
      expect(await session.sdk.storedFederations()).toEqual([])
    },
  )

  sdkTest(
    'joins once, and refuses a second join with AlreadyJoined',
    async ({ session, invite, federation }) => {
      const id = await federation.id()
      expect(id).toBe(await invite.federationId())
      expect(await federation.balance()).toBe(0n)
      await expect(session.sdk.join(invite)).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.AlreadyJoined,
      })
      const open = await session.sdk.federations()
      expect(open).toHaveLength(1)
      expect(await open[0].id()).toBe(id)
      const stored = await session.sdk.storedFederations()
      expect(stored).toHaveLength(1)
      expect(stored[0]).toMatchObject({ id, network: Network.Regtest })
      expect(stored[0].status.tag).toBe(FederationStatus_Tags.Running)
      expect((await session.sdk.federationStatus(id))?.tag).toBe(
        FederationStatus_Tags.Running,
      )
      expect(await session.sdk.federation(id)).toBeDefined()
    },
  )

  sdkTest(
    'describes itself: name, network, capabilities and invite code',
    async ({ session, invite, federation }) => {
      const preview = await session.sdk.preview(invite)
      expect(await federation.name()).toBe(preview.name)
      expect(await federation.network()).toBe(Network.Regtest)
      expect(await federation.capabilities()).toEqual({
        ecash: true,
        lightning: true,
        onchain: true,
      })
      const code = await federation.inviteCode()
      expect(await code.federationId()).toBe(await federation.id())
      expect(await code.display()).toMatch(/^fed1/)
      // Every facade the capabilities promise is there.
      expect(await federation.ecash()).toBeDefined()
      expect(await federation.lightning()).toBeDefined()
      expect(await federation.onchain()).toBeDefined()
    },
  )
})

describe('meta', () => {
  sdkTest(
    'reads the config metadata through every accessor',
    async ({ federation }) => {
      const meta = await federation.meta()
      const config = await meta.configMetadata()
      expect(config).toBeInstanceOf(Map)
      // `all` merges the override document into the config's; without one it is the config.
      const all = await meta.all()
      expect(all).toBeInstanceOf(Map)
      for (const key of config.keys()) {
        expect(all.has(key)).toBe(true)
        expect(await meta.get(key)).toBe(all.get(key))
      }
      expect(await meta.get('no-such-key-web-sdk-test')).toBeUndefined()
      const consensus = await meta.consensusMetadata()
      if (consensus !== undefined)
        expect(consensus.revision).toBeTypeOf('bigint')
    },
  )
})

describe('lifecycle', () => {
  sdkTest(
    'closes, reopens and forgets a federation',
    async ({ session, federation }) => {
      const id = await federation.id()
      const statuses = await session.sdk.federationStatusUpdates()

      await session.sdk.closeFederation(id)
      expect((await session.sdk.federationStatus(id))?.tag).toBe(
        FederationStatus_Tags.Closed,
      )
      expect(await session.sdk.federation(id)).toBeUndefined()
      expect(await session.sdk.federations()).toEqual([])
      // A handle taken before the close is not revived by anything.
      await expect(federation.balance()).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.FederationClosed,
      })
      const closed = await nextMatching(
        statuses,
        (info) =>
          info.id === id && info.status.tag === FederationStatus_Tags.Closed,
      )
      expect(closed.id).toBe(id)

      const reopened = await session.sdk.reopenFederation(id)
      expect(await reopened.id()).toBe(id)
      expect(await reopened.balance()).toBe(0n)
      expect((await session.sdk.federationStatus(id))?.tag).toBe(
        FederationStatus_Tags.Running,
      )
      await expect(federation.balance()).rejects.toMatchObject({
        code: ErrorCode.FederationClosed,
      })

      await session.sdk.forgetFederation(id)
      expect(await session.sdk.storedFederations()).toEqual([])
      expect(await session.sdk.federationStatus(id)).toBeUndefined()
      expect(await session.sdk.federation(id)).toBeUndefined()
    },
  )
})

describe('balance and activity', () => {
  sdkTest(
    'balance updates report the current balance, then each change',
    async ({ federation }) => {
      const updates = await federation.balanceUpdates()
      expect(await updates.next()).toBe(0n)
      const changed = updates.next()
      const balance = await fund(federation)
      const first = await changed
      expect(first).toBeGreaterThan(0n)
      expect(first).toBeLessThanOrEqual(balance)
    },
  )

  sdkTest(
    'an empty wallet has an empty activity page',
    async ({ federation }) => {
      const page = await federation.activity(undefined, 5)
      expect(page.items).toEqual([])
      expect(page.next).toBeUndefined()
    },
  )

  sdkTest(
    'activity lists the funding receive, and operation(id) opens it',
    async ({ funded }) => {
      const { federation } = funded
      const page = await federation.activity(undefined, 10)
      expect(page.next).toBeUndefined()
      expect(page.items).toHaveLength(1)
      const [item] = page.items
      expect(item).toMatchObject({
        kind: OperationKind.LnReceive,
        direction: Direction.Incoming,
        status: ActivityStatus.Success,
        isFinal: true,
      })
      expect(item.amount).toBeTypeOf('bigint')
      expect(item.time).toBeTypeOf('bigint')
      expect(item.operationId).toMatch(/^[0-9a-f]{64}$/)

      const any = await federation.operation(item.operationId)
      expect(any).toBeDefined()
      expect(await any!.id()).toBe(item.operationId)
      expect(await any!.kind()).toBe(OperationKind.LnReceive)
      expect(await any!.support()).toBe(OperationSupport.Observable)
      expect(await any!.supportedKind()).toBe(OperationKind.LnReceive)
      const receive = await any!.asLnReceive()
      expect(receive).toBeDefined()
      expect((await receive!.state()).tag).toBe(LnReceiveState_Tags.Claimed)
      expect(await any!.asEcashSend()).toBeUndefined()
      expect(await any!.asRecovery()).toBeUndefined()

      expect(await federation.operation('0'.repeat(64))).toBeUndefined()
    },
  )
})
