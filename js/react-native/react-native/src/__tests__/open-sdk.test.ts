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

  it('shuts the SDK down and lets go of it once, however often it is closed', async () => {
    // Destroying it is what hands the data directory back, so an application can close one
    // wallet and open another over the same directory.
    const session = await openSdk({ dataDir: '/data/fedimint' })
    await session.close()
    await session.close()
    expect(Sdk.instances[0]!.shutdowns).toBe(1)
    expect(Sdk.instances[0]!.destroys).toBe(1)
  })

  it('makes a later close wait for the first rather than resolving early', async () => {
    // `close` is what hands the data directory back, so a caller it resolves for has to be able
    // to open that directory again. A second call returning while the first is still tearing
    // down would hand back a session that has not let go of anything yet.
    const session = await openSdk({ dataDir: '/data/fedimint' })
    const sdk = Sdk.instances[0]!
    let release = () => {}
    sdk.shutdownGate = new Promise<void>((resolve) => {
      release = resolve
    })

    const first = session.close()
    const second = session.close()
    // What the second caller could see the moment its own close resolved: anything less than a
    // destroyed SDK means it was handed a session still holding the directory.
    let destroysWhenSecondResolved = -1
    const watch = second.then(() => {
      destroysWhenSecondResolved = sdk.destroys
    })

    release()
    await Promise.all([first, second, watch])
    expect(destroysWhenSecondResolved).toBe(1)
    expect(sdk.shutdowns).toBe(1)
    expect(sdk.destroys).toBe(1)
  })

  it('destroys the SDK even when the shutdown fails, and reports it to every caller', async () => {
    // A failed flush still leaves a closed instance, and nothing else would ever hand the data
    // directory back, so the destroy has to happen anyway.
    const session = await openSdk({ dataDir: '/data/fedimint' })
    Sdk.instances[0]!.failShutdown = new Error('the flush failed')
    await expect(session.close()).rejects.toThrow('the flush failed')
    await expect(session.close()).rejects.toThrow('the flush failed')
    expect(Sdk.instances[0]!.shutdowns).toBe(1)
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
