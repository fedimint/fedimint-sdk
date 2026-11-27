// Fixtures for the browser tests, which run against the federation devimint stands up (see
// scripts/setup_test_shell.sh and the `web-sdk` project of js/vitest.config.ts). Every test gets
// a session of its own, on a storage name nobody else uses, so tests neither share state nor
// contend for the same origin-private file.
//
// The package is imported through its `exports` (not a relative path into `src`), so the worker
// entry, the wasm-bindgen glue and the module are loaded from `dist`, exactly as an application
// loads them.
import { expect, test as base } from 'vitest'
import {
  LnReceiveState_Tags,
  openSdk,
  type FederationLike,
  type InviteCodeLike,
  type Proxied,
  type SdkSession,
} from '@fedimint/web-sdk'

const FAUCET = import.meta.env.FAUCET

// devimint's faucet: it hands out its federation's invite code, and pays and issues invoices
// through its gateway. Amounts are in millisatoshis.
async function call(path: string, body?: string): Promise<string> {
  const response = await fetch(
    `${FAUCET}${path}`,
    body === undefined ? undefined : { method: 'POST', body },
  )
  const text = await response.text()
  if (!response.ok)
    throw new Error(`faucet ${path} answered ${response.status}: ${text}`)
  return text.trim()
}

export const faucet = {
  inviteCode: () => call('/connect-string'),
  pay: (invoice: string) => call('/pay', invoice),
  invoice: (msats: bigint) => call('/invoice', msats.toString()),
}

/** A storage name no other test uses: letters, digits and `-`, well under the 64 char limit. */
export function uniqueStorage(label: string): string {
  const random = Math.random().toString(36).slice(2, 8)
  return `web-sdk-${label}-${Date.now().toString(36)}-${random}`
}

/** The amount every funded fixture receives, in millisatoshis. */
export const FUNDING = 100_000n

/**
 * Funds the federation's wallet over Lightning: an invoice the faucet pays, followed until the
 * receive is claimed. Returns the balance afterwards, which the gateway's fee can make a little
 * less than the invoice.
 */
export async function fund(
  federation: Proxied<FederationLike>,
  amount = FUNDING,
): Promise<bigint> {
  const lightning = await federation.lightning()
  expect(lightning, 'devimint runs a lightning module').toBeDefined()
  const receive = await lightning!.receive(amount, 'funding')
  await faucet.pay(receive.invoice)
  const final = await receive.operation.awaitFinal()
  expect(final.tag).toBe(LnReceiveState_Tags.Claimed)
  const balance = await federation.balance()
  expect(balance).toBeGreaterThan(0n)
  return balance
}

/** Reads `next()` until `accept` takes a value or `limit` values went by; returns the taken one. */
export async function nextMatching<T>(
  updates: { next(options?: { signal: AbortSignal }): Promise<T> },
  accept: (value: T) => boolean,
  limit = 10,
): Promise<T> {
  for (let i = 0; i < limit; i++) {
    const value = await updates.next()
    if (accept(value)) return value
  }
  throw new Error(`no matching update within ${limit} updates`)
}

/** Polls the balance until it drops below `before`, or fails after `timeoutMs`. */
export async function balanceBelow(
  federation: Proxied<FederationLike>,
  before: bigint,
  timeoutMs = 60_000,
): Promise<bigint> {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    const balance = await federation.balance()
    if (balance < before) return balance
    if (Date.now() >= deadline)
      throw new Error(`the balance never dropped below ${before}`)
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
}

export interface Fixtures {
  /** The storage name of `session`. */
  storage: string
  /** An open session over `storage`, closed after the test. Closing it earlier is fine. */
  session: SdkSession
  /** devimint's invite code, parsed in `session`. */
  invite: Proxied<InviteCodeLike>
  /** `session` joined to devimint's federation. */
  federation: Proxied<FederationLike>
  /** `federation`, funded with `FUNDING` over Lightning, and the balance after funding. */
  funded: { federation: Proxied<FederationLike>; balance: bigint }
}

export const sdkTest = base.extend<Fixtures>({
  // eslint-disable-next-line no-empty-pattern
  storage: async ({}, use) => {
    await use(uniqueStorage('test'))
  },
  session: async ({ storage }, use) => {
    const session = await openSdk({ storage })
    try {
      await session.sdk.setMnemonic(await session.Mnemonic.generate())
      await use(session)
    } finally {
      await session.close()
    }
  },
  invite: async ({ session }, use) => {
    await use(await session.InviteCode.parse(await faucet.inviteCode()))
  },
  federation: async ({ session, invite }, use) => {
    await use(await session.sdk.join(invite))
  },
  funded: async ({ federation }, use) => {
    const balance = await fund(federation)
    await use({ federation, balance })
  },
})

/** Opens another session for the duration of `run`, on a storage of its own unless given. */
export async function withSession<T>(
  label: string,
  run: (session: SdkSession, storage: string) => Promise<T>,
  options: { storage?: string; mnemonic?: string[] } = {},
): Promise<T> {
  const storage = options.storage ?? uniqueStorage(label)
  const session = await openSdk({ storage, mnemonic: options.mnemonic })
  try {
    if (!(await session.sdk.hasMnemonic())) {
      await session.sdk.setMnemonic(await session.Mnemonic.generate())
    }
    return await run(session, storage)
  } finally {
    await session.close()
  }
}
