import type { OnchainQuoteLike, Proxied } from '@fedimint/web-sdk'
import { useEffect, useState } from 'react'
import { msat, sat, time } from '../format'
import { useAction, useQuote, type OperationLike } from '../hooks'
import { useFederation } from '../sdk'
import { OperationState } from './OperationState'

/**
 * On-chain deposits and withdrawals: `onchain.receive`, `onchain.deposits`, `onchain.quote`,
 * `onchain.send`. A deposit becomes an operation once the wallet finds it.
 */
export function Onchain() {
  const federation = useFederation()
  const [address, setAddress] = useState('')
  const [deposit, setDeposit] = useState<OperationLike>()
  const [sendTo, setSendTo] = useState('')
  const [sats, setSats] = useState('50000')
  const input = JSON.stringify([sendTo.trim(), sats])
  const quoted = useQuote<Proxied<OnchainQuoteLike>>(input)
  const [operation, setOperation] = useState<OperationLike>()
  const { run, busy, error } = useAction()

  // Each deposit to one of this federation's addresses arrives as its own operation.
  useEffect(() => {
    if (!address) return
    const controller = new AbortController()
    const signal = controller.signal
    void (async () => {
      try {
        const onchain = await federation.onchain()
        if (!onchain) return
        const deposits = await onchain.deposits({ signal })
        for (;;) setDeposit(await deposits.next({ signal }))
      } catch (e) {
        if (!signal.aborted) console.error('deposits:', e)
      }
    })()
    return () => controller.abort()
  }, [federation, address])

  const handleAddress = () =>
    run(async () => {
      const onchain = await federation.onchain()
      if (!onchain) throw new Error('this federation has no on-chain module')
      setAddress(await onchain.receive())
    })

  const handleQuote = () =>
    run(async () => {
      const to = sendTo.trim()
      const onchain = await federation.onchain()
      if (!onchain) throw new Error('this federation has no on-chain module')
      const q = await onchain.quote(to, BigInt(sats))
      const [amount, fee, total, expiresAt] = await Promise.all([
        q.amount(),
        q.fee(),
        q.total(),
        q.expiresAt(),
      ])
      quoted.set(input, q, [
        ['To', to],
        ['Amount', sat(amount)],
        ['Fee', msat(fee)],
        ['Total', msat(total)],
        ['Expires', time(expiresAt)],
      ])
    })

  const handleSend = async (e: React.FormEvent) => {
    e.preventDefault()
    await run(async () => {
      const quote = quoted.quote
      if (!quote) throw new Error('get a quote first')
      const onchain = await federation.onchain()
      if (!onchain) throw new Error('this federation has no on-chain module')
      setOperation(await onchain.send(quote))
      quoted.clear()
    })
  }

  return (
    <div className="section">
      <h3>On-chain</h3>
      <h4>Receive</h4>
      <button type="button" onClick={handleAddress} disabled={busy}>
        New Deposit Address
      </button>
      {address && (
        <div className="success">
          <strong>Deposit address:</strong>
          <pre className="invoice-wrap">{address}</pre>
          <div className="muted">
            Watching for deposits. On regtest, mine blocks with devimint's
            bitcoin-cli after sending to it.
          </div>
        </div>
      )}
      <OperationState operation={deposit} />
      <h4>Send</h4>
      <form onSubmit={handleSend}>
        <div className="input-group">
          <label htmlFor="send-to">Address:</label>
          <input
            id="send-to"
            required
            value={sendTo}
            onChange={(e) => setSendTo(e.target.value)}
          />
        </div>
        <div className="input-group">
          <label htmlFor="sats">Amount (sats):</label>
          <input
            id="sats"
            type="number"
            required
            value={sats}
            onChange={(e) => setSats(e.target.value)}
          />
        </div>
        <div className="button-group">
          <button
            type="button"
            onClick={handleQuote}
            disabled={busy || !sendTo.trim()}
          >
            Quote
          </button>
          <button type="submit" disabled={busy || !quoted.quote}>
            {busy ? 'Working...' : 'Send'}
          </button>
        </div>
      </form>
      {quoted.info.length > 0 && (
        <div className="preview-result">
          <h4>Quote</h4>
          <div className="preview-info">
            {quoted.info.map(([label, value]) => (
              <div key={label}>
                <strong>{label}:</strong> {value}
              </div>
            ))}
          </div>
        </div>
      )}
      <OperationState operation={operation} />
      {error && <div className="error">{error}</div>}
    </div>
  )
}
