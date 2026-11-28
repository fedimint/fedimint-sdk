import {
  ActivityStatus,
  Direction,
  OperationKind,
  type ActivityItem,
  type ProxiedValue,
} from '@fedimint/web-sdk'
import { useState } from 'react'
import { msat, short, time } from '../format'
import { useAction } from '../hooks'
import { useFederation } from '../sdk'

const PAGE = 20

/**
 * The federation's history, newest first, a page at a time: `federation.activity`. An ecash send
 * shows its notes again from the SDK's record of it: `federation.operation`, `details`.
 */
export function Activity() {
  const federation = useFederation()
  const [items, setItems] = useState<ProxiedValue<ActivityItem>[]>([])
  const [cursor, setCursor] = useState<string>()
  const [loaded, setLoaded] = useState(false)
  const [sent, setSent] = useState<{
    id: string
    notes: string
    value: bigint
  }>()
  const { run, busy, error } = useAction()

  // `next` is the cursor for the page after this one; `undefined` means this was the last.
  const load = (fromStart: boolean) =>
    run(async () => {
      const page = await federation.activity(
        fromStart ? undefined : cursor,
        PAGE,
      )
      setItems(
        fromStart ? page.items : (previous) => [...previous, ...page.items],
      )
      setCursor(page.next)
      setLoaded(true)
    })

  // The section is mounted afresh for each federation, so `federation` is the one the row is from.
  const showNotes = (id: string) =>
    run(async () => {
      const send = await (await federation.operation(id))?.asEcashSend()
      if (!send)
        throw new Error(`no ecash send ${short(id)} in this federation`)
      const details = await send.details()
      setSent({
        id,
        notes: await details.notes.display(),
        value: details.notesValue,
      })
    })

  return (
    <div className="section">
      <h3>Activity</h3>
      <div className="button-group">
        <button type="button" onClick={() => load(true)} disabled={busy}>
          {loaded ? 'Reload' : 'Load'}
        </button>
        {cursor !== undefined && (
          <button type="button" onClick={() => load(false)} disabled={busy}>
            More
          </button>
        )}
      </div>
      {loaded && items.length === 0 && (
        <div className="muted">Nothing yet.</div>
      )}
      {items.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>Time</th>
              <th>Kind</th>
              <th>Direction</th>
              <th className="num">Amount</th>
              <th className="num">Fee</th>
              <th>Status</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.operationId}>
                <td>{time(item.time)}</td>
                <td>{OperationKind[item.kind]}</td>
                <td>
                  {item.direction === undefined
                    ? ''
                    : Direction[item.direction]}
                </td>
                <td className="num">
                  {item.amount === undefined ? '' : msat(item.amount)}
                </td>
                <td className="num">
                  {item.fee === undefined ? '' : msat(item.fee)}
                </td>
                <td>
                  {ActivityStatus[item.status]}
                  {item.isFinal ? '' : ' …'}
                </td>
                <td>
                  {item.kind === OperationKind.EcashSend && (
                    <button
                      type="button"
                      className="btn-small"
                      onClick={() => showNotes(item.operationId)}
                      disabled={busy}
                    >
                      Notes
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {sent && (
        <div className="success">
          <strong>
            Notes of send {short(sent.id)}, worth {msat(sent.value)}:
          </strong>
          <pre className="invoice-wrap">{sent.notes}</pre>
          <button
            type="button"
            onClick={() => navigator.clipboard.writeText(sent.notes)}
          >
            Copy
          </button>
        </div>
      )}
      {error && <div className="error">{error}</div>}
    </div>
  )
}
