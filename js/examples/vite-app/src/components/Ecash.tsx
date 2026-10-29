import {
  EcashSendState,
  type EcashQuoteLike,
  type Proxied,
} from '@fedimint/web-sdk'
import { useState } from 'react'
import { msat, time } from '../format'
import { useAction, useQuote, type OperationLike } from '../hooks'
import { useFederation, useSession } from '../sdk'
import { OperationState } from './OperationState'

const sendStateName = (n: number) => EcashSendState[n]

/** Sends ecash: `ecash.quote` fixes the notes and the fee, and `ecash.send` spends that quote. */
export function SendEcash() {
  const federation = useFederation()
  const [amount, setAmount] = useState('10000')
  const quoted = useQuote<Proxied<EcashQuoteLike>>(amount)
  const [notes, setNotes] = useState('')
  const [operation, setOperation] = useState<OperationLike>()
  const { run, busy, error } = useAction()

  const handleQuote = () =>
    run(async () => {
      const ecash = await federation.ecash()
      if (!ecash) throw new Error('this federation has no ecash module')
      const q = await ecash.quote(BigInt(amount))
      const [requested, notesValue, fee, total, expiresAt] = await Promise.all([
        q.requestedAmount(),
        q.notesValue(),
        q.fee(),
        q.total(),
        q.expiresAt(),
      ])
      quoted.set(amount, q, [
        ['Requested', msat(requested)],
        ['Notes value', msat(notesValue)],
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
      const ecash = await federation.ecash()
      if (!ecash) throw new Error('this federation has no ecash module')
      const sent = await ecash.send(quote)
      setNotes(await sent.notes.display())
      setOperation(sent.operation)
      quoted.clear()
    })
  }

  return (
    <div className="section">
      <h3>Send Ecash</h3>
      <form onSubmit={handleSend}>
        <div className="input-group">
          <label htmlFor="ecash-amount">Amount (msats):</label>
          <input
            id="ecash-amount"
            type="number"
            required
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </div>
        <div className="button-group">
          <button type="button" onClick={handleQuote} disabled={busy}>
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
      {notes && (
        <div className="success">
          <strong>Notes to hand over:</strong>
          <pre className="invoice-wrap">{notes}</pre>
          <button
            type="button"
            onClick={() => navigator.clipboard.writeText(notes)}
          >
            Copy
          </button>
        </div>
      )}
      <OperationState operation={operation} names={sendStateName} />
      {error && <div className="error">{error}</div>}
    </div>
  )
}

/** Receives ecash: `Notes.parse`, then `ecash.receive`. Your own notes reissue fine. */
export function RedeemEcash() {
  const session = useSession()
  const federation = useFederation()
  const [input, setInput] = useState('')
  const [value, setValue] = useState('')
  const [operation, setOperation] = useState<OperationLike>()
  const { run, busy, error } = useAction()

  const handleParse = () =>
    run(async () => {
      const notes = await session.Notes.parse(input.trim())
      setValue(`Worth ${msat(await notes.value())}`)
    })

  const handleRedeem = async (e: React.FormEvent) => {
    e.preventDefault()
    await run(async () => {
      const ecash = await federation.ecash()
      if (!ecash) throw new Error('this federation has no ecash module')
      const notes = await session.Notes.parse(input.trim())
      setOperation(await ecash.receive(notes))
    })
  }

  return (
    <div className="section">
      <h3>Redeem Ecash</h3>
      <form onSubmit={handleRedeem}>
        <textarea
          placeholder="Long ecash string..."
          required
          rows={3}
          value={input}
          onChange={(e) => {
            setInput(e.target.value)
            setValue('')
          }}
        />
        <div className="button-group">
          <button
            type="button"
            onClick={handleParse}
            disabled={busy || !input.trim()}
          >
            Parse
          </button>
          <button type="submit" disabled={busy || !input.trim()}>
            Redeem
          </button>
        </div>
      </form>
      {value && <div className="success">{value}</div>}
      <OperationState operation={operation} />
      {error && <div className="error">{error}</div>}
    </div>
  )
}
