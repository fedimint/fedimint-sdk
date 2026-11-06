/* eslint-disable no-console */
import { HarnessClient } from './client/HarnessClient'
import { FaucetClient } from './env/faucet'
import { fundWallet } from './fixtures/fundWallet'
import { joinFederation } from './fixtures/joinFederation'
import { openWallet } from './fixtures/openWallet'
import { Fixture } from './fixtures/types'
import { Method } from './protocol/methods'
import { selectTests } from './registry'
import { createTarget } from './targets'
import { Target } from './targets/types'
import { State, TestContext, TestDef } from './tests/types'

// Keyed by the state each produces; `planFixtures` walks `requires` from
// here, so a test naming "funded" gets open, join and fund without saying so.
const FIXTURES: Record<State, Fixture> = {
  walletOpen: openWallet,
  joinedFederation: joinFederation,
  funded: fundWallet,
}

function planFixtures(
  needed: readonly State[],
  have: ReadonlySet<State>,
): Fixture[] {
  const plan: Fixture[] = []
  const planned = new Set<State>()
  const visiting = new Set<State>()
  const visit = (state: State): void => {
    if (have.has(state) || planned.has(state)) return
    if (visiting.has(state)) throw new Error(`Cyclic fixture at "${state}"`)
    visiting.add(state)
    FIXTURES[state].requires.forEach(visit)
    visiting.delete(state)
    planned.add(state)
    plan.push(FIXTURES[state])
  }
  needed.forEach(visit)
  return plan
}

/** Every method a test needs, its fixtures' included. */
function methodsNeeded(test: TestDef): Set<Method> {
  const methods = new Set<Method>(test.requires)
  for (const fixture of planFixtures(test.prerequisites, new Set())) {
    fixture.methods.forEach((m) => methods.add(m))
  }
  return methods
}

/**
 * Checks the harness against what the selected tests need before any runs.
 * A test needing a method the harness lacks is skipped only when the target
 * says why; otherwise the run fails here, so coverage is never lost quietly.
 */
async function preflight(
  h: HarnessClient,
  target: Target,
  tests: TestDef[],
): Promise<TestDef[]> {
  const { methods } = await h.call('harness.describe', {})
  const runnable: TestDef[] = []
  const problems: string[] = []
  for (const test of tests) {
    const testSkip = target.knownUnsupported[test.name]
    if (testSkip) {
      console.log(`SKIPPED ${test.name}: ${testSkip}`)
      continue
    }
    const missing = [...methodsNeeded(test)].filter((m) => !methods.includes(m))
    const unexplained = missing.filter((m) => !target.knownUnsupported[m])
    if (unexplained.length > 0) {
      problems.push(`${test.name} needs ${unexplained.join(', ')}`)
    } else if (missing.length > 0) {
      const why = missing.map((m) => `${m}: ${target.knownUnsupported[m]}`)
      console.log(`SKIPPED ${test.name}: ${why.join('; ')}`)
    } else {
      runnable.push(test)
    }
  }
  if (problems.length > 0) {
    throw new Error(
      `The ${target.name} harness lacks methods the selected tests need:\n  ` +
        problems.join('\n  '),
    )
  }
  return runnable
}

async function main(args: string[]): Promise<boolean> {
  const tests = selectTests(args)
  const target = createTarget()
  const ran: string[] = []
  const results: { name: string; error?: unknown }[] = []
  // Mutated as fixtures and tests run; cleared by a reset.
  const state = new Set<State>()
  // Skipped tests (known unsupported on this target) are not owed a pass.
  let expected = tests.length

  try {
    const backend = await target.start()
    const h = new HarnessClient(backend)
    const ctx: TestContext = {
      h,
      target,
      faucet: new FaucetClient(),
      log: (line) => console.log(line),
    }

    const runnable = await preflight(h, target, tests)
    expected = runnable.length
    console.log(
      `Running on ${target.name}: ${runnable.map((t) => t.name).join(', ')}`,
    )

    for (const test of runnable) {
      console.log(`\n=== ${test.name} ===`)
      try {
        // Anything held beyond what the test wants is reset away, so each
        // test sees exactly the state it declares.
        const wanted = new Set(test.prerequisites)
        if ([...state].some((s) => !wanted.has(s))) {
          console.log(`Resetting: holding [${[...state].join(', ')}]`)
          await target.reset()
          state.clear()
        }
        for (const fixture of planFixtures(test.prerequisites, state)) {
          await fixture.run(ctx)
          state.add(fixture.produces)
        }
        ran.push(test.name)
        await test.run(ctx)
        test.produces.forEach((s) => state.add(s))
        results.push({ name: test.name })
        console.log(`=== ${test.name} passed ===`)
      } catch (error) {
        results.push({ name: test.name, error })
        console.error(
          `=== ${test.name} failed: ${(error as Error).message} ===`,
        )
        await target.captureFailure(test.name)
        // Fail fast: against a federation, the next test would fail on the
        // same broken state, for a reason already captured.
        break
      }
    }
  } catch (error) {
    results.push({ name: 'setup', error })
    console.error(`Run failed: ${(error as Error).stack ?? error}`)
  }

  const problems = await target
    .afterRun(ran)
    .catch((e: Error) => [`After-run checks failed: ${e.message}`])
  await target.stop()

  console.log('\n=== Summary ===')
  for (const r of results) {
    console.log(
      `${r.name}: ${r.error ? `FAILED — ${(r.error as Error).message}` : 'passed'}`,
    )
  }
  problems.forEach((p) => console.error(`PROBLEM: ${p}`))
  const passed = results.filter((r) => !r.error).length
  console.log(`${passed}/${expected} tests passed.`)
  return passed === expected && problems.length === 0
}

main(process.argv.slice(2)).then(
  (ok) => process.exit(ok ? 0 : 1),
  (error) => {
    console.error(error)
    process.exit(1)
  },
)
