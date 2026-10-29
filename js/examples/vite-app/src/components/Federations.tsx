import {
  Network,
  type FederationInfo,
  type ProxiedValue,
} from '@fedimint/web-sdk'
import { useEffect, useState } from 'react'
import { errorText, short, variant } from '../format'
import { useAction } from '../hooks'
import { useSdk, useSession } from '../sdk'

/**
 * The federations this wallet knows, closed ones included, with the one in use highlighted:
 * `sdk.storedFederations`, `sdk.federation`, `sdk.reopenFederation`,
 * `sdk.federationStatusUpdates`.
 */
export function Federations() {
  const session = useSession()
  const { federationId, version, selectFederation, federationsChanged } =
    useSdk()
  const [stored, setStored] = useState<ProxiedValue<FederationInfo>[]>([])
  const { run, busy, error, setError } = useAction()

  useEffect(() => {
    let cancelled = false
    session.sdk
      .storedFederations()
      .then((list) => {
        if (!cancelled) setStored(list)
      })
      .catch((e) => setError(errorText(e)))
    return () => {
      cancelled = true
    }
  }, [session, version, setError])

  // A status changes without the page asking: a recovery finishes, a federation is closed.
  // Each `next()` waits for the next change.
  useEffect(() => {
    const controller = new AbortController()
    void (async () => {
      try {
        const updates = await session.sdk.federationStatusUpdates()
        for (;;) {
          await updates.next({ signal: controller.signal })
          federationsChanged()
        }
      } catch (e) {
        if (!controller.signal.aborted) console.error('status updates:', e)
      }
    })()
    return () => controller.abort()
  }, [session, federationsChanged])

  const select = (id: string) =>
    run(async () => {
      // `federation(id)` only hands out open federations; a closed one is reopened first.
      const federation =
        (await session.sdk.federation(id)) ??
        (await session.sdk.reopenFederation(id))
      await selectFederation(federation)
      federationsChanged()
    })

  if (stored.length === 0) return null

  return (
    <div className="section">
      <h3>Federations ({stored.length})</h3>
      <div className="wallet-grid">
        {stored.map((info) => (
          <div
            key={info.id}
            className={`wallet-item ${info.id === federationId ? 'active' : ''}`}
          >
            <button
              className="wallet-button"
              disabled={busy}
              onClick={() => select(info.id)}
              title={info.id}
            >
              <div className="wallet-info">
                <strong>{info.name ?? 'unnamed'}</strong>
                <div className="wallet-federation">
                  {short(info.id)} · {Network[info.network]}
                </div>
                <div className="wallet-status">{variant(info.status)}</div>
              </div>
            </button>
          </div>
        ))}
      </div>
      {error && <div className="error">{error}</div>}
    </div>
  )
}
