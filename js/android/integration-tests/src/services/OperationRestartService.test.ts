/* eslint-disable no-console */
import {
  AppiumTestBase,
  NETWORK_TIMEOUT,
} from '../configs/appium/AppiumTestBase'
import {
  lookupOperation,
  operationIdFrom,
  payFromOutside,
  readBalanceMsats,
  restartAndReopen,
  stateFrom,
  waitForBalanceAbove,
} from '../flows/wallet'

const RECEIVE_MSATS = 30_000

// An operation outliving the process that started it: issue an invoice, kill
// the app before it is paid, reopen, find the receive again by its id through
// `federation.operation`, and watch the reattached handle carry it through to
// `Claimed` once the invoice is paid. The persisted operation, not the app's
// memory, is what has to finish the job.
export class OperationRestartService extends AppiumTestBase {
  // `funded` is not needed, only tolerated: declaring it lets a run of `all`
  // continue on the previous test's wallet instead of resetting the app.
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
    console.log('Starting OperationRestartService test')

    await this.scrollToElement('lnReceiveAmount')
    await this.typeIntoElementByKey('lnReceiveAmount', String(RECEIVE_MSATS))
    await this.typeIntoElementByKey('lnReceiveDescription', 'android restart')
    await this.dismissKeyboard()
    await this.clickElementByKey('lnReceive')
    const issued = await this.waitForTextInElement(
      'lnReceiveResult',
      'Invoice:',
      NETWORK_TIMEOUT,
    )
    const invoice = issued.match(/ln\w+/i)?.[0]
    if (!invoice) throw new Error(`No bolt11 invoice in: "${issued}"`)
    const id = operationIdFrom(issued)
    console.log(`Receive ${id} issued ${invoice.slice(0, 24)}…`)

    await restartAndReopen(this)
    const before = await readBalanceMsats(this)

    const record = await lookupOperation(this, id)
    if (!record.includes('kind LN_RECEIVE')) {
      throw new Error(`Expected an LN_RECEIVE record for ${id}: "${record}"`)
    }
    const state = stateFrom(record)
    if (!['Created', 'WaitingForPayment'].includes(state)) {
      throw new Error(
        `An unpaid receive should still be waiting after a restart, reports "${state}"`,
      )
    }
    console.log(`Reattached to ${id} in ${state}`)

    await payFromOutside(invoice)

    // The lookup section keeps following the reattached handle's updates.
    const claimed = await this.waitForTextInElement(
      'operationResult',
      'state: Claimed',
      NETWORK_TIMEOUT,
    )
    console.log(claimed.replace(/\n/g, ' | '))

    const after = await waitForBalanceAbove(this, before)
    if (after - before > RECEIVE_MSATS) {
      throw new Error(
        `Received ${after - before} msat for a ${RECEIVE_MSATS} msat invoice`,
      )
    }

    console.log('OperationRestartService test passed')
  }
}
