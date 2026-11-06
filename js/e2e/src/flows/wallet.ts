/* eslint-disable no-console */
import { execSync } from 'child_process'

import { NETWORK_TIMEOUT } from '../env/constants'
import { FaucetClient } from '../env/faucet'
import { ObservableKind, StateOf } from '../protocol/values'
import { TestContext } from '../tests/types'

// Ways of driving the harness that fixtures and tests share, so the two can
// never disagree about what "funded" means.

/**
 * Waits for the balance to read exactly `expectedMsats`: first as the
 * `balanceUpdates()` stream delivered it, then through a one-shot `balance()`,
 * so both paths are held to the same number.
 */
export async function waitForBalance(
  { h }: TestContext,
  expectedMsats: number,
  timeoutMs = NETWORK_TIMEOUT,
): Promise<void> {
  await h.poll(
    'federation.observedBalance',
    {},
    (b) => (b.amountMsats === expectedMsats ? true : undefined),
    { timeoutMs, what: `the observed balance to reach ${expectedMsats} msat` },
  )
  const { amountMsats } = await h.call('federation.balance', {})
  if (amountMsats !== expectedMsats) {
    throw new Error(
      `balanceUpdates() delivered ${expectedMsats} msat but balance() reads ${amountMsats}`,
    )
  }
}

/**
 * Waits for the stream to deliver a balance above `previousMsats` and returns
 * it. Because it reads what `balanceUpdates()` delivered, a receive that
 * settles here proves the subscription works across the binding, not only
 * that a one-shot call returns.
 */
export async function waitForBalanceAbove(
  { h }: TestContext,
  previousMsats: number,
  timeoutMs = NETWORK_TIMEOUT,
): Promise<number> {
  return h.poll(
    'federation.observedBalance',
    {},
    (b) =>
      b.amountMsats !== null && b.amountMsats > previousMsats
        ? b.amountMsats
        : undefined,
    {
      timeoutMs,
      what: `the observed balance to rise above ${previousMsats} msat`,
    },
  )
}

/**
 * Waits for the watcher of an operation of `kind` to record a state whose
 * type is one of `types`, and returns it. Fails early when the operation is of
 * another kind, ends in another state, or its watcher reports an error.
 */
export async function waitForState<
  K extends ObservableKind,
  T extends StateOf<K>['type'],
>(
  { h }: TestContext,
  kind: K,
  operationId: string,
  types: readonly T[],
  timeoutMs = NETWORK_TIMEOUT,
): Promise<Extract<StateOf<K>, { type: T }>> {
  return h.poll(
    'operation.observed',
    { operationId },
    (o) => {
      if (o.kind !== kind) {
        throw new Error(`Operation ${operationId} is ${o.kind}, not ${kind}`)
      }
      const last = o.states[o.states.length - 1] as StateOf<K> | undefined
      if (last && (types as readonly string[]).includes(last.type)) {
        return last as Extract<StateOf<K>, { type: T }>
      }
      if (o.error) {
        throw new Error(
          `Operation ${operationId} reported [${o.error.code}] ${o.error.reason}`,
        )
      }
      if (o.final) {
        throw new Error(
          `Operation ${operationId} ended in ${last?.type} instead of ${types.join(' or ')}`,
        )
      }
      return undefined
    },
    {
      timeoutMs,
      what: `operation ${operationId} to reach ${types.join(' or ')}`,
    },
  )
}

/**
 * The funding path, end to end: the harness issues an invoice, a devimint
 * node pays it, the operation is claimed and the balance rises. Returns the
 * balance once the receive has settled.
 */
export async function receiveOverLightning(
  ctx: TestContext,
  msats: number,
  description = 'e2e',
): Promise<number> {
  const { h, log } = ctx
  const before = (await h.call('federation.balance', {})).amountMsats
  const { invoice, operationId } = await h.call('lightning.receive', {
    amountMsats: msats,
    description,
  })
  log(`[flow] invoice for ${msats} msat: ${invoice.slice(0, 24)}…`)
  await payFromOutside(invoice)
  await waitForState(ctx, 'LN_RECEIVE', operationId, ['CLAIMED'])
  const after = await waitForBalanceAbove(ctx, before)
  log(`[flow] balance ${before} -> ${after} msat`)
  return after
}

/** How many of the latest `limit` activity rows are of `kind`. */
export async function countActivity(
  { h }: TestContext,
  kind: string,
  limit = 100,
): Promise<number> {
  const { items } = await h.call('activity.list', { limit })
  return items.filter((item) => item.kind === kind).length
}

/**
 * Pays an invoice the harness issued, from a lightning node that is not the
 * one that issued it.
 *
 * The SDK's `receive` takes no gateway, and devimint runs two (LND and LDK),
 * so which one issued the invoice is not ours to choose. The faucet always
 * pays from the LDK node, which cannot pay an invoice its own gateway issued,
 * so on refusal this pays from LND instead: exactly one of the two is never
 * the issuer.
 */
export async function payFromOutside(invoice: string): Promise<void> {
  try {
    await new FaucetClient().payInvoice(invoice)
    return
  } catch (error) {
    console.log(
      `[flow] the faucet's LDK node would not pay this invoice ` +
        `(${(error as Error).message.split('\n')[0]}), trying LND`,
    )
  }

  const lncli = process.env.FM_LNCLI
  if (!lncli) {
    throw new Error(
      'The faucet could not pay the invoice and FM_LNCLI is not set, so there ' +
        'is no second node to try. Is this running under scripts/setup_test_shell.sh?',
    )
  }
  // FM_LNCLI is a command line, not a path, so it goes through a shell. The
  // invoice came from the harness, so it is checked rather than trusted.
  if (!/^ln[a-z0-9]+$/i.test(invoice)) {
    throw new Error(`Refusing to shell out with "${invoice}" as an invoice`)
  }
  execSync(`${lncli} payinvoice --force --json ${invoice}`, {
    stdio: 'pipe',
    timeout: NETWORK_TIMEOUT,
  })
}
