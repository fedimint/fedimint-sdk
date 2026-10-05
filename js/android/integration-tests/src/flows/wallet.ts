/* eslint-disable no-console */
import { execSync } from 'child_process'

import {
  AppiumTestBase,
  DEFAULT_TIMEOUT,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import { FaucetClient } from '../faucet/FaucetClient'

// Shared ways of driving the example app, used by both fixtures and tests so the
// two can never disagree about what "funded" means. The wasm suite keeps the
// same thing in TestFedimintWallet (`fundWallet`).

/**
 * Reads the balance line as millisatoshis.
 *
 * The example app renders whole sats when the amount is round and
 * `"<sats> sat (<msats> msat)"` when it isn't, so the msat form wins when
 * present and the sat form is scaled when it isn't.
 */
export async function readBalanceMsats(t: AppiumTestBase): Promise<number> {
  // The balance line sits at the top of the example app's one long screen,
  // under the wallet status, and a test that has scrolled down to a section
  // below it — the ecash sections are two screens down — leaves it off
  // screen. UiAutomator2 reports only what is on screen, so the element is
  // then absent from the tree rather than present, and reading it blind fails
  // against an app that is working fine. The same rule `waitForTextInElement`
  // follows: a miss means "scroll to it", not "it isn't there".
  if (!(await t.findElementByKey('balance'))) {
    await t.scrollToElement('balance', { scrollDirection: 'up' })
  }
  // Right after a federation is attached — a join, or an open that
  // reattached after a restart — the line reads "Balance: …" until the first
  // `balance()` returns, so that placeholder is waited out, briefly.
  const deadline = Date.now() + DEFAULT_TIMEOUT
  let text = await t.getTextByKey('balance')
  while (text.includes('…') && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 500))
    text = await t.getTextByKey('balance')
  }
  const msats = parseMsats(text)
  if (msats === null) {
    throw new Error(`Could not read a balance out of "${text}"`)
  }
  return msats
}

/**
 * Reads an amount the example app formatted with its `formatMsats` — whole
 * sats when the amount is round, `"<sats> sat (<msats> msat)"` when it isn't —
 * out of `text`, as millisatoshis. The msat form wins when present. Returns
 * null when there is no amount in it.
 */
export function parseMsats(text: string): number | null {
  const msats = text.match(/\((\d+) msat\)/)
  if (msats) return Number(msats[1])

  const sats = text.match(/([\d,]+)\s*sat/)
  if (sats) return Number(sats[1].replace(/,/g, '')) * 1000

  return null
}

/**
 * Reads the amount on the line of a quote that starts with `label`
 * ("Amount", "Fee", "Total"), as millisatoshis.
 */
export function quoteLineMsats(quote: string, label: string): number {
  const line = quote.split('\n').find((l) => l.trim().startsWith(label))
  const msats = line === undefined ? null : parseMsats(line)
  if (msats === null) {
    throw new Error(`No "${label}" amount in the quote: "${quote}"`)
  }
  return msats
}

/**
 * Waits for the balance line to read exactly `expectedMsats`.
 *
 * For a spend or a credit whose exact effect is known: the SDK quotes the
 * total a send debits, and a claimed deposit reports its net credit. Polled,
 * because the change a send's funding transaction mints, or a deposit's claim,
 * lands after the operation itself reports success.
 */
export async function waitForBalance(
  t: AppiumTestBase,
  expectedMsats: number,
  timeout = NETWORK_TIMEOUT,
): Promise<void> {
  const startTime = Date.now()
  let last = -1
  while (Date.now() - startTime < timeout) {
    last = await readBalanceMsats(t)
    if (last === expectedMsats) return
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }
  throw new Error(
    `Balance never settled at ${expectedMsats} msat within ${timeout}ms — last read ${last} msat`,
  )
}

/** A structured SDK error, as the example app renders an `SdkException`. */
export interface SdkError {
  code: string
  reason: string
}

/**
 * Pulls the `[<ErrorCode>] <reason>` line the example app writes for every
 * `SdkException` out of a result line, or null when there is none. The code is
 * the stable `ErrorCode` the SDK returned, which is what an application
 * branches on; the friendly sentence above it is the app's own copy.
 */
export function readSdkError(text: string): SdkError | null {
  const match = text.match(/\[([A-Z_]+)\] ?([\s\S]*)$/)
  return match ? { code: match[1], reason: match[2].trim() } : null
}

/** Asserts that a result line carries the structured SDK error `code`. */
export function expectSdkError(text: string, code: string): SdkError {
  const error = readSdkError(text)
  if (!error || error.code !== code) {
    throw new Error(`Expected an SDK error [${code}], got: "${text}"`)
  }
  return error
}

/** The first operation id named in a result line ("operation <id>"). */
export function operationIdFrom(text: string): string {
  const id = text.match(/operation ([0-9a-f]{16,})/i)?.[1]
  if (!id) throw new Error(`No operation id in: "${text}"`)
  return id
}

/** The `state: …` an operation's result line currently shows. */
export function stateFrom(text: string): string {
  const states = [...text.matchAll(/state: (.+)/g)]
  if (states.length === 0) throw new Error(`No operation state in: "${text}"`)
  return states[states.length - 1][1].trim()
}

/**
 * Reattaches to an operation by id through the "Look Up Operation" section,
 * which calls `federation.operation(id)`, downcasts it and follows its typed
 * state from there. Returns the section's first outcome: the kind and state,
 * "not found", or an SDK error. Follow later states with
 * `waitForTextInElement('operationResult', …)`.
 */
export async function lookupOperation(
  t: AppiumTestBase,
  id: string,
): Promise<string> {
  await t.scrollToElement('operationId')
  await t.typeIntoElementByKey('operationId', id)
  await t.dismissKeyboard()
  await t.clickElementByKey('operationLookup')
  return t.waitForResultInElement('operationResult', NETWORK_TIMEOUT)
}

/** The joined federation's id, from the `id <hex>` line of the status. */
export async function readFederationId(t: AppiumTestBase): Promise<string> {
  if (!(await t.findElementByKey('walletStatus'))) {
    await t.scrollToElement('walletStatus', { scrollDirection: 'up' })
  }
  const status = await t.getTextByKey('walletStatus')
  const id = status.match(/\bid ([0-9a-f]{64})\b/)?.[1]
  if (!id) throw new Error(`No federation id in the status line: "${status}"`)
  return id
}

/** The seed's words, through "Show seed" (`exportMnemonic().words()`). */
export async function readSeedWords(t: AppiumTestBase): Promise<string[]> {
  // The toggle's label says which way it will go; the seed view's absence
  // would not, since an off-screen view is absent too.
  await t.scrollToElement('toggleSeed')
  if (/show/i.test(await t.getTextByKey('toggleSeed'))) {
    await t.clickElementByKey('toggleSeed')
  }
  await t.scrollToElement('seed')
  const seed = await t.getTextByKey('seed')
  const words = seed
    .trim()
    .split(/\s+/)
    .filter((token) => token.length > 0 && !/^\d+\.$/.test(token))
  // Hide it again, so the screen is as the next step expects it.
  await t.clickElementByKey('toggleSeed')
  return words
}

/**
 * Restarts the app without clearing its data and opens the SDK again over
 * the same storage. Returns the open's result line, which names the
 * federation it reattached to, if any.
 */
export async function restartAndReopen(t: AppiumTestBase): Promise<string> {
  await t.restartApp()
  await t.clickElementByKey('openWallet')
  return t.waitForTextInElement('walletResult', 'opened', NETWORK_TIMEOUT)
}

/**
 * Waits for the balance line to report more than `previousMsats`.
 *
 * The example app keeps this line live off `federation.balanceUpdates()`, so a
 * receive that settles shows up here without the test tapping anything —
 * which is the point: it proves the subscription delivers over the FFI
 * boundary, not just that a one-shot call returns.
 */
export async function waitForBalanceAbove(
  t: AppiumTestBase,
  previousMsats: number,
  timeout = NETWORK_TIMEOUT,
): Promise<number> {
  const startTime = Date.now()
  let last = previousMsats
  while (Date.now() - startTime < timeout) {
    last = await readBalanceMsats(t)
    if (last > previousMsats) return last
    await new Promise((resolve) => setTimeout(resolve, 1000))
  }
  throw new Error(
    `Balance never rose above ${previousMsats} msat within ${timeout}ms — last read ${last} msat`,
  )
}

/**
 * The funding path, end to end: the app issues an invoice, the devimint
 * faucet pays it, and the balance rises.
 *
 * Mirrors the wasm suite's `fundWallet`, with the app standing in for direct
 * SDK calls. Returns the balance once the receive has settled.
 */
export async function receiveOverLightning(
  t: AppiumTestBase,
  msats: number,
  description = 'android e2e',
): Promise<number> {
  const before = await readBalanceMsats(t)

  await t.scrollToElement('lnReceiveAmount')
  await t.typeIntoElementByKey('lnReceiveAmount', String(msats))
  await t.typeIntoElementByKey('lnReceiveDescription', description)
  await t.dismissKeyboard()
  await t.clickElementByKey('lnReceive')

  // The result line opens with "Invoice:\n<bolt11>", then the operation's
  // state updates overwrite it — so read the invoice out while it is there.
  const result = await t.waitForTextInElement(
    'lnReceiveResult',
    'Invoice:',
    NETWORK_TIMEOUT,
  )
  const invoice = result.match(/ln\w+/i)?.[0]
  if (!invoice) {
    throw new Error(`No bolt11 invoice in the result line: "${result}"`)
  }
  console.log(`[flow] invoice for ${msats} msat: ${invoice.slice(0, 24)}…`)

  await payFromOutside(invoice)

  const after = await waitForBalanceAbove(t, before)
  console.log(`[flow] balance ${before} -> ${after} msat`)
  return after
}

/**
 * Pays an invoice the app issued, from a lightning node that is not the one
 * that issued it.
 *
 * Which node issued it is not ours to choose. The SDK's `receive` takes no
 * gateway — fedimint picks one by vetting and fee, and devimint runs two
 * (LND and LDK) whose ids are fresh each run, so the tiebreak between two equal,
 * unvetted gateways is effectively arbitrary from here. The faucet always
 * pays from the LDK node (devimint/src/faucet.rs), so when fedimint happens
 * to pick the LDK gateway, the faucet would be paying an invoice its own node
 * issued, which lightning does not do.
 *
 * The wasm suite sidesteps this by naming the gateway when it creates the
 * invoice (`createInvoice(..., info)` with the faucet's advertised LND
 * gateway). An app driven through its UI cannot, so pay from the other node
 * instead: devimint exports a ready-made `lncli` invocation, and exactly one
 * of the two payers is never the issuer.
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
  // FM_LNCLI is a command line, not a path, so it has to go through a shell.
  // The invoice is checked rather than trusted: it comes back off a screen.
  if (!/^ln[a-z0-9]+$/i.test(invoice)) {
    throw new Error(`Refusing to shell out with "${invoice}" as an invoice`)
  }
  execSync(`${lncli} payinvoice --force --json ${invoice}`, {
    stdio: 'pipe',
    timeout: NETWORK_TIMEOUT,
  })
}
