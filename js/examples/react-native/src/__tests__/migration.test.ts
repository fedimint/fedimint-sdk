import { readFileSync } from 'node:fs'
import ts from 'typescript'
import { describe, expect, it, vi } from 'vitest'

const guide = readFileSync(
  new URL('../../../../../docs/react-native/migration.md', import.meta.url),
  'utf8',
)
const snippet = guide
  .split('```ts [setup.ts]\n')[1]!
  .split('\n```')[0]!
  .replace(/^import .*$/gm, '')
const code = ts.transpileModule(snippet, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.None,
  },
}).outputText
const run = new (Object.getPrototypeOf(async () => undefined).constructor)(
  'openSdk',
  'dataDir',
  'choice',
  `${code}\nreturn 'configured'`,
) as (open: unknown, dir: string, choice: unknown) => Promise<string>

describe('migration guide setup', () => {
  it('validates explicit Restore on seeded storage and stops on SeedMismatch', async () => {
    const fromWords = vi.fn((words: string[]) => ({ words }))
    const setMnemonic = vi.fn(async () => {
      throw new Error('SeedMismatch')
    })
    const openSdk = async () => ({
      sdk: { hasMnemonic: () => true, setMnemonic },
      Mnemonic: { fromWords },
    })
    await expect(
      run(openSdk, '/wallet', { kind: 'restore', words: ['other', 'words'] }),
    ).rejects.toThrow('SeedMismatch')
    expect(fromWords).toHaveBeenCalledWith(['other', 'words'])
    expect(setMnemonic).toHaveBeenCalledTimes(1)
  })

  it('guards Create on an existing wallet without generating new words', async () => {
    const generate = vi.fn()
    const setMnemonic = vi.fn()
    const openSdk = async () => ({
      sdk: { hasMnemonic: () => true, setMnemonic },
      Mnemonic: { generate },
    })
    expect(await run(openSdk, '/wallet', { kind: 'create' })).toBe('configured')
    expect(generate).not.toHaveBeenCalled()
    expect(setMnemonic).not.toHaveBeenCalled()
  })
})
