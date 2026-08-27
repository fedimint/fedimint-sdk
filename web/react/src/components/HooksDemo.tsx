import React, { useState } from 'react'
import {
  useBalance,
  useReceiveLightning,
  useOpenWallet,
  useSendLightning,
  useSpendEcash,
  useReceiveEcash,
} from '../../lib'

const TEST_FEDERATION_INVITE =
  'fed11qgqzc2nhwden5te0vejkg6tdd9h8gepwvejkg6tdd9h8garhduhx6at5d9h8jmn9wshxxmmd9uqqzgxg6s3evnr6m9zdxr6hxkdkukexpcs3mn7mj3g5pc5dfh63l4tj6g9zk4er'

function HooksDemo() {
  const [bolt11Input, setBolt11Input] = useState<string>('')
  const [ecashAmount, setEcashAmount] = useState<string>('')
  const [ecashInput, setEcashInput] = useState<string>('')

  // Balance
  const balance = useBalance()

  // Wallet
  const { walletStatus, openWallet, joinFederation } = useOpenWallet()
  const isOpen = walletStatus === 'open'

  // Receive Lightning
  const { generateInvoice, bolt11, invoiceStatus, error } =
    useReceiveLightning()

  // Send Lightning
  const { payInvoice, payment, paymentStatus, paymentError } =
    useSendLightning()

  // SendEcash
  const {
    spendEcash,
    notes,
    state: spendEcashState,
    error: spendEcashError,
  } = useSpendEcash()

  // ReceiveEcash
  const {
    redeemEcash,
    state: receiveEcashState,
    error: receiveEcashError,
  } = useReceiveEcash()

  return (
    <>
      <div className="card">
        <h2>Hooks</h2>
        <div className="section">
          <b>useOpenWallet()</b>
          <div className="row">
            <b>walletStatus</b>
            <p>{walletStatus}</p>
          </div>
          <div className="row">
            <b>openWallet()</b>
            <button disabled={isOpen} onClick={openWallet}>
              Open Wallet
            </button>
          </div>
          <div className="row">
            <b>joinFederation(invite)</b>
            <button
              disabled={isOpen}
              onClick={() => joinFederation(TEST_FEDERATION_INVITE)}
            >
              Join Federation
            </button>
          </div>
        </div>

        <div className="section">
          <b>useBalance()</b>
          <div className="row">
            <b>useBalance</b>
            <p>{balance ? `${balance / 1000} sats` : 'no balance'}</p>
          </div>
        </div>

        <div className="section">
          <b>useLightningInvoice()</b>
          <div className="row">
            <b>generateInvoice(1000, '1000 msats')</b>
            <button
              disabled={!isOpen}
              onClick={() =>
                generateInvoice(1_000, 'invoice from fedimint web sdk')
              }
            >
              Generate Invoice
            </button>
          </div>
          <div className="row">
            <b>bolt11</b>
            <p className="truncate">
              {bolt11 ? bolt11 : 'no invoice generated'}
            </p>
          </div>
          <div className="row">
            <b>invoiceStatus</b>
            <p>
              {typeof invoiceStatus === 'string'
                ? invoiceStatus
                : typeof invoiceStatus === 'object'
                  ? Object.keys(invoiceStatus)[0]
                  : 'no invoice status'}
            </p>
          </div>
          <div className="row">
            <b>error</b>
            <p>{error}</p>
          </div>
        </div>
        <div className="section">
          <b>useSendLightning()</b>
          <div className="row">
            <b>bolt11</b>
            <input
              type="text"
              value={bolt11Input}
              onChange={(e) => setBolt11Input(e.target.value)}
            />
          </div>
          <div className="row">
            <b>payInvoice(bolt11)</b>
            <button
              disabled={!isOpen || !bolt11Input}
              onClick={() => bolt11Input && payInvoice(bolt11Input)}
            >
              Pay Invoice
            </button>
          </div>
          <div className="row">
            <b>paymentStatus</b>
            <p>
              {typeof paymentStatus === 'string'
                ? paymentStatus
                : typeof paymentStatus === 'object'
                  ? Object.keys(paymentStatus)[0]
                  : 'no payment status'}
            </p>
          </div>
          <div className="row">
            <b>paymentError</b>
            <p>{paymentError}</p>
          </div>
        </div>
        <div className="section">
          <b>useSpendEcash()</b>
          <div className="row">
            <b>amount sats</b>
            <input
              type="number"
              value={ecashAmount}
              onChange={(e) => setEcashAmount(e.target.value)}
            />
          </div>
          <div className="row">
            <b>spendEcash(amount)</b>
            <button
              disabled={!isOpen || !ecashAmount}
              onClick={() => ecashAmount && spendEcash(parseInt(ecashAmount))}
            >
              Spend Ecash
            </button>
          </div>
          <div className="row">
            <b>ecashStatus</b>
            <p>
              {typeof spendEcashState === 'string'
                ? spendEcashState
                : typeof spendEcashState === 'object'
                  ? Object.keys(spendEcashState)[0]
                  : 'no ecash status'}
            </p>
          </div>
          <div className="row">
            <b>ecashNote</b>
            <p title={notes} className="truncate">
              {notes}
            </p>
          </div>
          <div className="row">
            <b>error</b>
            <p>{spendEcashError}</p>
          </div>
        </div>
        <div className="section">
          <b>useReceiveEcash()</b>
          <div className="row">
            <b>Ecash string</b>
            <input
              type="string"
              value={ecashInput}
              onChange={(e) => setEcashInput(e.target.value)}
            />
          </div>
          <div className="row">
            <b>redeemEcash(notes)</b>
            <button
              disabled={!isOpen || !ecashInput}
              onClick={() => ecashInput && redeemEcash(ecashInput)}
            >
              Redeem Ecash
            </button>
          </div>
          <div className="row">
            <b>ecashRedeemStatus</b>
            <p>
              {typeof receiveEcashState === 'string'
                ? receiveEcashState
                : typeof receiveEcashState === 'object'
                  ? Object.keys(receiveEcashState)[0]
                  : 'no ecash status'}
            </p>
          </div>
          <div className="row">
            <b>error</b>
            <p>{receiveEcashError}</p>
          </div>
        </div>
      </div>
    </>
  )
}

export default HooksDemo
