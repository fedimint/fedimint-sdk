import type { LnQuoteLike, Proxied } from '@fedimint/web-sdk'
import { useState } from 'react'
import { faucet } from '../faucet'
import { msat, time, variant } from '../format'
import { useAction, useQuote, type OperationLike } from '../hooks'
import { useFederation, useSdk } from '../sdk'
import { OperationState } from './OperationState'

/** Creates an invoice and follows its payment: `lightning.receive`. */
export function GenerateLightningInvoice() {
  const federation = useFederation()
  const { faucetUrl } = useSdk()
  const [amount, setAmount] = useState('100000')
  const [description, setDescription] = useState('vite example')
  const [invoice, setInvoice] = useState('')
  const [operation, setOperation] = useState<OperationLike>()
  const [paid, setPaid] = useState('')
  const { run, busy, error } = useAction()

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setPaid('')
    await run(async () => {
      const lightning = await federation.lightning()
      if (!lightning) throw new Error('this federation has no lightning module')
      const receive = await lightning.receive(BigInt(amount), description)
      setInvoice(receive.invoice)
      setOperation(receive.operation)
    })
  }

  const handleFaucetPay = () =>
    run(async () => {
      await faucet.pay(faucetUrl, invoice)
      setPaid('The faucet paid the invoice')
    })

  return (
    <div className="section">
      <h3>Generate Lightning Invoice</h3>
      <form onSubmit={handleSubmit}>
        <div className="input-group">
          <label htmlFor="amount">Amount (msats):</label>
          <input
            id="amount"
            type="number"
            required
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </div>
        <div className="input-group">
          <label htmlFor="description">Description:</label>
          <input
            id="description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>
        <button type="submit" disabled={busy}>
          {busy ? 'Generating...' : 'Generate Invoice'}
        </button>
      </form>
      {invoice && (
        <div className="success">
          <strong>Generated Invoice:</strong>
          <pre className="invoice-wrap">{invoice}</pre>
          <div className="button-group">
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(invoice)}
            >
              Copy
            </button>
            <button type="button" onClick={handleFaucetPay} disabled={busy}>
              Pay it from the faucet
            </button>
          </div>
        </div>
      )}
      <OperationState operation={operation} />
      {paid && <div className="success">{paid}</div>}
      {error && <div className="error">{error}</div>}
    </div>
  )
}

/** Pays an invoice: `lightning.quote`, which picks the route and fixes the fee, then `send`. */
export function SendLightning() {
  const federation = useFederation()
  const { faucetUrl } = useSdk()
  const [invoice, setInvoice] = useState('')
  const [faucetAmount, setFaucetAmount] = useState('50000')
  const input = invoice.trim()
  const quoted = useQuote<Proxied<LnQuoteLike>>(input)
  const [operation, setOperation] = useState<OperationLike>()
  const { run, busy, error } = useAction()

  const handleFaucetInvoice = () =>
    run(async () => {
      setInvoice(await faucet.invoice(faucetUrl, BigInt(faucetAmount)))
    })

  const handleQuote = () =>
    run(async () => {
      const lightning = await federation.lightning()
      if (!lightning) throw new Error('this federation has no lightning module')
      const q = await lightning.quote(input)
      const [invoiceAmount, fee, route, total, expiresAt] = await Promise.all([
        q.invoiceAmount(),
        q.fee(),
        q.route(),
        q.total(),
        q.expiresAt(),
      ])
      quoted.set(input, q, [
        ['Invoice amount', msat(invoiceAmount)],
        ['Fee', msat(fee)],
        ['Route', variant(route)],
        ['Total', msat(total)],
        ['Expires', time(expiresAt)],
      ])
    })

  const handlePay = async (e: React.FormEvent) => {
    e.preventDefault()
    await run(async () => {
      const quote = quoted.quote
      if (!quote) throw new Error('get a quote first')
      const lightning = await federation.lightning()
      if (!lightning) throw new Error('this federation has no lightning module')
      setOperation(await lightning.send(quote))
      quoted.clear()
    })
  }

  return (
    <div className="section">
      <h3>Pay Lightning</h3>
      <form onSubmit={handlePay}>
        <textarea
          placeholder="lnbc..."
          required
          rows={3}
          value={invoice}
          onChange={(e) => setInvoice(e.target.value)}
        />
        <div className="button-group">
          <button type="button" onClick={handleQuote} disabled={busy || !input}>
            Quote
          </button>
          <button type="submit" disabled={busy || !quoted.quote}>
            {busy ? 'Working...' : 'Pay'}
          </button>
        </div>
      </form>
      <div className="input-group">
        <label htmlFor="faucet-amount">Faucet invoice (msats):</label>
        <input
          id="faucet-amount"
          type="number"
          value={faucetAmount}
          onChange={(e) => setFaucetAmount(e.target.value)}
        />
        <button type="button" onClick={handleFaucetInvoice} disabled={busy}>
          Get one
        </button>
      </div>
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
