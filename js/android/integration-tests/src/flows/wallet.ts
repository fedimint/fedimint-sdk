/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import { FaucetClient } from '../faucet/FaucetClient'

// Shared ways of driving the demo app, used by both fixtures and tests so the
// two can never disagree about what "funded" means. The wasm suite keeps the
// same thing in TestFedimintWallet (`fundWallet`).

/**
 * Reads the balance line as millisatoshis.
 *
 * The demo renders whole sats when the amount is round and
 * `"<sats> sat (<msats> msat)"` when it isn't, so the msat form wins when
 * present and the sat form is scaled when it isn't.
 */
export async function readBalanceMsats(t: AppiumTestBase): Promise<number> {
  const text = await t.getTextByKey('balance')

  const msats = text.match(/\((\d+) msat\)/)
  if (msats) return Number(msats[1])

  const sats = text.match(/([\d,]+)\s*sat/)
  if (sats) return Number(sats[1].replace(/,/g, '')) * 1000

  throw new Error(`Could not read a balance out of "${text}"`)
}

/**
 * Waits for the balance line to report more than `previousMsats`.
 *
 * The demo keeps this line live off `federation.balanceUpdates()`, so a
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

  await new FaucetClient().payInvoice(invoice)

  const after = await waitForBalanceAbove(t, before)
  console.log(`[flow] balance ${before} -> ${after} msat`)
  return after
}
