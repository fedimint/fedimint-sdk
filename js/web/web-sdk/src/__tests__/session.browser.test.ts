import { describe, expect } from 'vitest'
import { ErrorCode, FederationStatus_Tags, openSdk } from '@fedimint/web-sdk'
import { fund, sdkTest, uniqueStorage, withSession } from './fixtures'

// What crosses the worker boundary before anything is joined: opening and closing, the storage
// claim, the seed, and the errors each of them reports.
describe('session', () => {
  sdkTest(
    'opens over an empty storage with nothing joined',
    async ({ session }) => {
      expect(await session.sdk.federations()).toEqual([])
      expect(await session.sdk.storedFederations()).toEqual([])
    },
  )

  sdkTest(
    'two sessions on different storages run side by side',
    async ({ session }) => {
      await withSession('second', async (second) => {
        const a = await (await session.sdk.exportMnemonic()).words()
        const b = await (await second.sdk.exportMnemonic()).words()
        expect(a).toHaveLength(12)
        expect(b).toHaveLength(12)
        expect(a).not.toEqual(b)
      })
    },
  )

  sdkTest(
    'a storage that is open elsewhere is refused with StorageInUse',
    async ({ storage, session }) => {
      // `session` holds the storage open. The browser hands out one sync access handle per
      // file, so the second worker's open fails, and `openSdk` terminates that worker rather
      // than leaving it behind.
      expect(await session.sdk.federations()).toEqual([])
      await expect(openSdk({ storage })).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.StorageInUse,
      })
    },
  )

  sdkTest(
    'a closed session rejects a call, and closing twice is a no-op',
    async () => {
      const session = await openSdk({ storage: uniqueStorage('closed') })
      const sdk = session.sdk
      await session.close()
      await expect(sdk.federations()).rejects.toMatchObject({
        name: 'SessionClosed',
      })
      await expect(session.close()).resolves.toBeUndefined()
    },
  )

  sdkTest(
    'closing a session fails a subscription that is waiting',
    async ({ session, federation }) => {
      const updates = await federation.balanceUpdates()
      // The first `next()` answers with the current balance; the second waits for a change.
      expect(await updates.next()).toBe(0n)
      const waiting = updates.next()
      await session.close()
      // Either the shutdown ends the subscription in the worker, which answers with an SDK error,
      // or the worker is terminated first, which fails every call in flight with SessionClosed.
      const error = await waiting.then(
        () => undefined,
        (e: unknown) => e,
      )
      expect(error).toBeInstanceOf(Error)
      expect(['SessionClosed', 'SdkError']).toContain((error as Error).name)
    },
  )

  sdkTest(
    'an abort ends a waiting call, and an aborted signal is refused up front',
    async ({ federation }) => {
      const updates = await federation.balanceUpdates()
      expect(await updates.next()).toBe(0n)
      const controller = new AbortController()
      const waiting = updates.next({ signal: controller.signal })
      setTimeout(() => controller.abort(), 200)
      await expect(waiting).rejects.toMatchObject({ name: 'AbortError' })
      // An already-aborted signal never fires `abort`, so the client refuses it without a round
      // trip to the worker.
      await expect(
        updates.next({ signal: AbortSignal.abort() }),
      ).rejects.toMatchObject({ name: 'AbortError' })
      // The subscription itself is intact: the balance is still readable through it after a
      // change.
      const changed = updates.next()
      await fund(federation)
      expect(await changed).toBeGreaterThan(0n)
    },
  )

  sdkTest(
    'a funded wallet comes back after a restart',
    async ({ storage, session, federation }) => {
      const balance = await fund(federation)
      const id = await federation.id()
      await session.close()
      const again = await openSdk({ storage })
      try {
        const federations = await again.sdk.federations()
        expect(federations).toHaveLength(1)
        expect(await federations[0].id()).toBe(id)
        expect(await federations[0].balance()).toBe(balance)
        const [stored] = await again.sdk.storedFederations()
        expect(stored.id).toBe(id)
        expect(stored.status.tag).toBe(FederationStatus_Tags.Running)
      } finally {
        await again.close()
      }
    },
  )
})

describe('seed', () => {
  sdkTest(
    'exports a 12 word mnemonic that a new storage can start from',
    async ({ session }) => {
      const words = await (await session.sdk.exportMnemonic()).words()
      expect(words).toHaveLength(12)
      await withSession(
        'same-seed',
        async (second) => {
          expect(await (await second.sdk.exportMnemonic()).words()).toEqual(
            words,
          )
        },
        { mnemonic: words },
      )
    },
  )

  sdkTest(
    'keeps its seed across a restart, and refuses another one',
    async ({ storage, session }) => {
      const words = await (await session.sdk.exportMnemonic()).words()
      const other = await (await session.Mnemonic.generate()).words()
      await session.close()
      await expect(openSdk({ storage, mnemonic: other })).rejects.toMatchObject(
        {
          name: 'SdkError',
          code: ErrorCode.SeedMismatch,
        },
      )
      const again = await openSdk({ storage })
      try {
        expect(await (await again.sdk.exportMnemonic()).words()).toEqual(words)
      } finally {
        await again.close()
      }
    },
  )

  sdkTest(
    'Mnemonic.generate and fromWords round trip, and bad words are refused',
    async ({ session }) => {
      const words = await (await session.Mnemonic.generate()).words()
      expect(words).toHaveLength(12)
      expect(await (await session.Mnemonic.fromWords(words)).words()).toEqual(
        words,
      )
      await expect(
        session.Mnemonic.fromWords(['not', 'a', 'mnemonic']),
      ).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.InvalidInput,
      })
    },
  )
})

describe('parsing', () => {
  sdkTest(
    'an invalid invite code and invalid notes fail with InvalidInput',
    async ({ session }) => {
      await expect(
        session.InviteCode.parse('not-an-invite'),
      ).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.InvalidInput,
      })
      await expect(session.Notes.parse('not-notes')).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.InvalidInput,
      })
    },
  )

  sdkTest(
    'an SdkError carries its code, reason and name across the worker',
    async ({ session }) => {
      const error = await session.InviteCode.parse('not-an-invite').catch(
        (e) => e,
      )
      expect(error).toBeInstanceOf(Error)
      expect(error.name).toBe('SdkError')
      expect(error.code).toBe(ErrorCode.InvalidInput)
      expect(typeof error.reason).toBe('string')
      expect(error.reason.length).toBeGreaterThan(0)
      expect(error.message).toBe(error.reason)
    },
  )
})
