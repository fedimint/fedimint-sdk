import { Method } from '../protocol/methods'
import { State, TestContext } from '../tests/types'

/**
 * Drives the harness from a fresh install to a named state. `requires` lists
 * the states that must be reached first; the runner orders fixtures so a test
 * names only the state it needs. `methods` are the protocol methods it calls.
 */
export interface Fixture {
  produces: State
  requires: readonly State[]
  methods: readonly Method[]
  run(ctx: TestContext): Promise<void>
}
