/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import {
  getNewAddress,
  mineBlocks,
  receivedByAddress,
  satsToBtc,
  sendToAddress,
} from '../devimint/bitcoind'
import { sleep } from '../devimint/env'
import {
  quoteLineMsats,
  readBalanceMsats,
  waitForBalance,
} from '../flows/wallet'
import { currentShape } from '../shape'

const DEPOSIT_SATS = 100_000
const WITHDRAW_SATS = 30_000
/** A deposit needs blocks mined and the federation to agree on them. */
const ONCHAIN_TIMEOUT = 180_000

// Bitcoin in and out through the wallet module (wallet on v1, walletv2 on
// v2): fund a fresh deposit address from devimint's bitcoind, mine it in,
// watch the claim credit the balance, then withdraw to a bitcoind address and
// find the coins there. The same round trip as the Rust
// `onchain_deposit_and_withdrawal_round_trip`, through the app.
//
// The device reaches the federation's esplora through the port
// run-android-e2e.sh forwards for it.
export class OnchainService extends AppiumTestBase {
  // `funded` is tolerated rather than needed, so a run of `all` keeps going
  // on the previous test's wallet; the deposit funds this test itself.
  static prerequisites: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]
  static produces: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]

  async execute(): Promise<void> {
    console.log('Starting OnchainService test')
    const shape = currentShape()

    // ── Deposit ──────────────────────────────────────────────────────────
    const before = await readBalanceMsats(this)
    await this.clickElementByKey('deposit')
    const issued = await this.waitForTextInElement(
      'depositResult',
      'Send bitcoin to:',
      NETWORK_TIMEOUT,
    )
    const address = issued.split('\n')[1]?.trim()
    if (!address || !/^bcrt1[0-9a-z]+$/.test(address)) {
      throw new Error(`No regtest deposit address in: "${issued}"`)
    }
    const txid = sendToAddress(address, DEPOSIT_SATS)
    console.log(`Sent ${DEPOSIT_SATS} sat to ${address} in ${txid}`)
    mineBlocks(1)

    // v1 reports the pending confirmation explicitly; walletv2 never does,
    // because its scanner only reports a deposit once it has claimed it.
    if (shape === 'v1') {
      await this.waitForTextInElement(
        'depositResult',
        `WaitingForConfirmation(txid=${txid}`,
        ONCHAIN_TIMEOUT,
      )
      console.log('v1 reported the deposit waiting for confirmations')
    }

    mineBlocks(21)
    const claimed = await this.waitForTextInElement(
      'depositResult',
      'state: Claimed',
      ONCHAIN_TIMEOUT,
    )
    const claim = claimed.match(
      /Claimed\(txid=([0-9a-f]{64}), gross=(\d+) sat, net=(\d+) msat\)/,
    )
    if (!claim) throw new Error(`Unreadable claim: "${claimed}"`)
    const [, claimedTxid, gross, net] = claim
    const netCredit = Number(net)
    if (claimedTxid !== txid) {
      throw new Error(`Claimed ${claimedTxid}, deposited in ${txid}`)
    }
    if (Number(gross) !== DEPOSIT_SATS) {
      throw new Error(
        `Claimed a gross ${gross} sat of a ${DEPOSIT_SATS} sat deposit`,
      )
    }
    if (netCredit <= 0 || netCredit > DEPOSIT_SATS * 1000) {
      throw new Error(`Net credit ${netCredit} msat for ${DEPOSIT_SATS} sat`)
    }
    // `net_credit` is defined as the amount the balance moved by.
    await waitForBalance(this, before + netCredit, ONCHAIN_TIMEOUT)
    console.log(`Deposit claimed: +${netCredit} msat`)

    // ── Withdrawal ───────────────────────────────────────────────────────
    const funded = before + netCredit
    const destination = getNewAddress()
    await this.scrollToElement('onchainAmount')
    await this.typeIntoElementByKey('onchainAmount', String(WITHDRAW_SATS))
    await this.typeIntoElementByKey('onchainAddress', destination)
    await this.dismissKeyboard()
    await this.clickElementByKey('onchainQuote')
    const quote = await this.waitForResultInElement(
      'onchainSendResult',
      NETWORK_TIMEOUT,
    )
    if (!quote.includes('Total')) {
      throw new Error(`Quoting the withdrawal failed: "${quote}"`)
    }
    const amount = quoteLineMsats(quote, 'Amount')
    const fee = quoteLineMsats(quote, 'Fee')
    const total = quoteLineMsats(quote, 'Total')
    if (amount !== WITHDRAW_SATS * 1000 || total !== amount + fee) {
      throw new Error(`Inconsistent withdrawal quote: "${quote}"`)
    }
    console.log(`Withdrawal quoted: ${amount} + ${fee} fee = ${total} msat`)

    await this.clickElementByKey('onchainSend')
    const sent = await this.waitForTextInElement(
      'onchainSendResult',
      'Succeeded(txid=',
      ONCHAIN_TIMEOUT,
    )
    const withdrawalTxid = sent.match(/Succeeded\(txid=([0-9a-f]{64})\)/)?.[1]
    if (!withdrawalTxid) throw new Error(`Unreadable withdrawal: "${sent}"`)
    console.log(`Withdrawal broadcast in ${withdrawalTxid}`)

    // `Succeeded` is the federation's broadcast; the transaction reaches
    // bitcoind's mempool a moment later, so the destination is polled
    // unconfirmed first, then checked again once a block confirms it.
    const expected = satsToBtc(WITHDRAW_SATS)
    const deadline = Date.now() + NETWORK_TIMEOUT
    while (receivedByAddress(destination, 0) !== expected) {
      if (Date.now() > deadline) {
        throw new Error(
          `The withdrawal ${withdrawalTxid} never reached ${destination}`,
        )
      }
      await sleep(1000)
    }
    mineBlocks(1)
    const confirmed = receivedByAddress(destination, 1)
    if (confirmed !== expected) {
      throw new Error(
        `${destination} holds ${confirmed} BTC confirmed, expected ${expected}`,
      )
    }

    // Exactly the quoted total, once the change is minted.
    await waitForBalance(this, funded - total, ONCHAIN_TIMEOUT)
    console.log(`Balance ${funded} -> ${funded - total} msat`)

    console.log('OnchainService test passed')
  }
}
