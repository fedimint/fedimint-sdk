/* eslint-disable no-console */
import { execSync } from 'child_process'

import {
  AppiumTestBase,
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
async function payFromOutside(invoice: string): Promise<void> {
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
