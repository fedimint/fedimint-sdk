import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  openSdk: vi.fn(),
  generate: vi.fn(),
  remove: vi.fn(),
}))
vi.mock('@fedimint/react-native', () => ({
  openSdk: mocks.openSdk,
  Mnemonic: { generate: mocks.generate },
  ErrorCode: {},
  Exception: {},
}))
vi.mock('react-native-fs', () => ({
  default: {
    DocumentDirectoryPath: '/documents',
    exists: async () => true,
    unlink: mocks.remove,
  },
}))
vi.mock('expo-file-system', () => ({
  Paths: { document: '/documents' },
  Directory: class {
    uri = 'file:///documents/fedimint'
    exists = true
    delete() {
      mocks.remove()
    }
  },
}))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

function session() {
  let closed = false
  let seed: string[] | undefined
  return {
    sdk: {
      hasMnemonic: vi.fn(() => {
        if (closed) throw new Error('destroyed handle')
        return !!seed
      }),
      setMnemonic: vi.fn(async (mnemonic: { words: string[] }) => {
        if (closed) throw new Error('destroyed handle')
        seed = mnemonic.words
      }),
      exportMnemonic: () => ({ words: () => seed }),
      federations: () => [],
    },
    Mnemonic: { fromWords: (words: string[]) => ({ words }) },
    close: vi.fn(async () => {
      closed = true
    }),
  }
}

for (const [name, load] of [
  ['React Native', () => import('../sdk')],
  ['Expo', () => import('../../../expo-app/src/sdk')],
] as const) {
  describe(`${name} restore handoff`, () => {
    beforeEach(() => {
      vi.resetModules()
      mocks.openSdk.mockReset()
      mocks.generate.mockReset()
      mocks.remove.mockClear()
    })

    it('does not publish a pending startup session after Restore supersedes it', async () => {
      const pending = deferred<ReturnType<typeof session>>()
      const old = session()
      const restored = session()
      mocks.openSdk
        .mockReturnValueOnce(pending.promise)
        .mockResolvedValueOnce(restored)
      const app = await load()
      const seen: unknown[] = []
      app.onChange(() => {
        const current = app.current()
        current?.sdk.hasMnemonic() // Throws if reset leaves a destroyed session published.
        seen.push(current)
      })
      const startup = app.open().catch((error: unknown) => error)
      await vi.waitFor(() => expect(mocks.openSdk).toHaveBeenCalledTimes(1))
      const reset = app.reset()
      const restore = app.open(['restored', 'words'])
      pending.resolve(old)
      await reset
      expect(await startup).toBeInstanceOf(Error)
      expect(await restore).toBe(restored)
      expect(old.close).toHaveBeenCalledTimes(1)
      expect(seen.filter(Boolean)).toEqual([restored])
      expect(restored.sdk.exportMnemonic().words()).toEqual([
        'restored',
        'words',
      ])
      expect(mocks.generate).not.toHaveBeenCalled()
    })

    it('does not delete data when the pending opener fails', async () => {
      const pending = deferred<ReturnType<typeof session>>()
      mocks.openSdk.mockReturnValueOnce(pending.promise)
      const app = await load()
      const startup = app.open().catch((error: unknown) => error)
      await vi.waitFor(() => expect(mocks.openSdk).toHaveBeenCalledTimes(1))
      const reset = expect(app.reset()).rejects.toThrow('StorageInUse')
      pending.reject(new Error('StorageInUse'))
      await reset
      expect(await startup).toBeInstanceOf(Error)
      expect(mocks.remove).not.toHaveBeenCalled()
      expect(app.current()).toBeUndefined()

      const retried = session()
      mocks.openSdk.mockResolvedValueOnce(retried)
      expect(await app.open()).toBe(retried)
      expect(mocks.openSdk).toHaveBeenCalledTimes(2)
      expect(app.current()).toBe(retried)
    })

    it('waits for the previous close before opening restored storage', async () => {
      const closing = deferred<void>()
      const old = session()
      old.close.mockImplementationOnce(() => closing.promise)
      const restored = session()
      mocks.openSdk.mockResolvedValueOnce(old).mockResolvedValueOnce(restored)
      const app = await load()
      await app.open()
      const reset = app.reset()
      const restore = app.open(['restored', 'words'])
      await vi.waitFor(() => expect(old.close).toHaveBeenCalledTimes(1))
      expect(mocks.openSdk).toHaveBeenCalledTimes(1)
      expect(app.current()).toBeUndefined()
      closing.resolve()
      await reset
      expect(await restore).toBe(restored)
      expect(app.current()).toBe(restored)
    })
  })
}
