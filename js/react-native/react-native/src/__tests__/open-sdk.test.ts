import { beforeEach, describe, expect, it } from 'vitest'

import { openSdk } from '../index'
import { InviteCode, Mnemonic, Notes, Sdk } from './bindings-stub'

describe('openSdk', () => {
  beforeEach(() => {
    Sdk.instances = []
    Sdk.failNext = undefined
    Mnemonic.fromWordsCalls = []
    Mnemonic.generated = 0
    InviteCode.parsed = []
    Notes.parsed = []
  })

  it('creates the SDK over the directory with no mnemonic when none is given', async () => {
    const session = await openSdk({ dataDir: '/data/fedimint' })
    expect(Sdk.instances).toHaveLength(1)
    expect(Sdk.instances[0]!.dataDir).toBe('/data/fedimint')
    expect(Sdk.instances[0]!.mnemonic).toBeUndefined()
    expect(session.sdk).toBe(Sdk.instances[0])
  })

  it('turns given words into a Mnemonic before creating the SDK', async () => {
    const words = ['abandon', 'ability', 'able']
    await openSdk({ dataDir: '/data/fedimint', mnemonic: words })
    expect(Mnemonic.fromWordsCalls).toEqual([words])
    expect(Sdk.instances[0]!.mnemonic?.words).toEqual(words)
  })

  it('forwards the static parsers and generators', async () => {
    const session = await openSdk({ dataDir: '/data/fedimint' })
    session.InviteCode.parse('fed11...')
    session.Notes.parse('notes...')
    session.Mnemonic.fromWords(['a'])
    session.Mnemonic.generate()
    expect(InviteCode.parsed).toEqual(['fed11...'])
    expect(Notes.parsed).toEqual(['notes...'])
    expect(Mnemonic.fromWordsCalls).toEqual([['a']])
    expect(Mnemonic.generated).toBe(1)
  })

  it('shuts the SDK down and lets go of it, and a second close does nothing', async () => {
    // Destroying it is what hands the data directory back, so an application can close one
    // wallet and open another over the same directory.
    const session = await openSdk({ dataDir: '/data/fedimint' })
    await session.close()
    await session.close()
    expect(Sdk.instances[0]!.shutdowns).toBe(1)
    expect(Sdk.instances[0]!.destroys).toBe(1)
  })

  it('destroys the SDK even when the shutdown fails', async () => {
    // A failed flush still leaves a closed instance, and nothing else would ever hand the data
    // directory back: `close` has already marked the session closed and will not run again.
    const session = await openSdk({ dataDir: '/data/fedimint' })
    Sdk.instances[0]!.failShutdown = new Error('the flush failed')
    await expect(session.close()).rejects.toThrow('the flush failed')
    expect(Sdk.instances[0]!.destroys).toBe(1)
  })

  it('rejects when the SDK cannot be created', async () => {
    Sdk.failNext = new Error('storage locked')
    await expect(openSdk({ dataDir: '/data/fedimint' })).rejects.toThrow(
      'storage locked',
    )
    expect(Sdk.instances).toHaveLength(0)
  })
})
