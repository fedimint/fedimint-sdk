import { consoleTest } from './tests/console.test'
import { errorsTest } from './tests/errors.test'
import { federationTest } from './tests/federation.test'
import { inviteCodeTest } from './tests/inviteCode.test'
import { lightningTest } from './tests/lightning.test'
import { lightningSendTest } from './tests/lightningSend.test'
import { mintTest } from './tests/mint.test'
import { mnemonicTest } from './tests/mnemonic.test'
import { onchainTest } from './tests/onchain.test'
import { operationRestartTest } from './tests/operationRestart.test'
import { persistenceTest } from './tests/persistence.test'
import { TestDef } from './tests/types'

/**
 * Every test, in the order `all` runs them. `console` comes first because
 * every other test depends on the channel it checks; after it, the order lets
 * each test inherit the state the last one left (open, joined, funded) rather
 * than resetting. The runner enforces states either way: the order only
 * decides how much work it repeats. The restart tests come after everything
 * that only reads the wallet, and the on-chain round trip, the slowest, last.
 */
export const TESTS: readonly TestDef[] = [
  consoleTest,
  mnemonicTest,
  inviteCodeTest,
  federationTest,
  lightningTest,
  mintTest,
  lightningSendTest,
  errorsTest,
  persistenceTest,
  operationRestartTest,
  onchainTest,
]

/** Resolves CLI arguments (names, or `all`) to tests, failing on an unknown
 * name rather than quietly testing less than asked. */
export function selectTests(args: readonly string[]): TestDef[] {
  if (args.includes('all')) return [...TESTS]
  const unknown = args.filter((a) => !TESTS.some((t) => t.name === a))
  if (unknown.length > 0 || args.length === 0) {
    throw new Error(
      `Unknown or no tests (${unknown.join(', ') || 'none'}). ` +
        `Available: ${TESTS.map((t) => t.name).join(', ')}, all`,
    )
  }
  return args.map((a) => TESTS.find((t) => t.name === a) as TestDef)
}
