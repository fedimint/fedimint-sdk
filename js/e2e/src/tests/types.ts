import { HarnessClient } from '../client/HarnessClient'
import { FaucetClient } from '../env/faucet'
import { Method } from '../protocol/methods'
import { Target } from '../targets/types'

/** A state of the harness a fixture builds and a test can require. */
export type State = 'walletOpen' | 'joinedFederation' | 'funded'

export interface TestContext {
  h: HarnessClient
  target: Target
  faucet: FaucetClient
  log: (line: string) => void
}

/**
 * One test, written once for every target. It throws to fail.
 *
 * `prerequisites` are built by fixtures from a fresh install before it runs;
 * `produces` is what it leaves behind, so the runner knows when the next test
 * needs a reset. `requires` lists the methods it calls, checked against the
 * harness before anything runs.
 */
export interface TestDef {
  name: string
  prerequisites: readonly State[]
  produces: readonly State[]
  requires: readonly Method[]
  run(ctx: TestContext): Promise<void>
}
