import {
  ActivityStatus,
  Direction,
  OperationKind,
  type ActivityItem,
  type ProxiedValue,
} from '@fedimint/web-sdk'
import { useState } from 'react'
import { msat, time } from '../format'
import { useAction } from '../hooks'
import { useFederation } from '../sdk'

const PAGE = 20

/** The federation's history, newest first, a page at a time: `federation.activity`. */
export function Activity() {
  const federation = useFederation()
  const [items, setItems] = useState<ProxiedValue<ActivityItem>[]>([])
  const [cursor, setCursor] = useState<string>()
  const [loaded, setLoaded] = useState(false)
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
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {error && <div className="error">{error}</div>}
    </div>
  )
}
