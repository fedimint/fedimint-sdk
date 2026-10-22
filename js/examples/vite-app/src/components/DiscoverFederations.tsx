import { DiscoverySource, Network } from '@fedimint/web-sdk'
import { useEffect, useState } from 'react'
import { errorText, short } from '../format'
import { useAction } from '../hooks'
import { useSession } from '../sdk'

/** A federation a relay announced, with its invite code read out of the worker. */
interface Discovered {
  id: string
  name: string | undefined
  network: Network | undefined
  modules: string[]
  invite: string
}

/**
 * Finds federations that announce themselves on Nostr (NIP-87): `defaultDiscoveryRelays` and
 * `sdk.discoverFederations`. Picking one hands its invite code to `onPick`.
 */
export function DiscoverFederations({
  onPick,
}: {
  onPick(invite: string): void
}) {
  const session = useSession()
  const [relays, setRelays] = useState('')
  const [found, setFound] = useState<Discovered[]>()
  const { run, busy, error, setError } = useAction()

  // The relays field starts with the SDK's own list, which an edit replaces.
  useEffect(() => {
    let cancelled = false
    session
      .defaultDiscoveryRelays()
      .then((list) => {
        if (!cancelled) setRelays((typed) => typed || list.join(' '))
      })
      .catch((e) => setError(errorText(e)))
    return () => {
      cancelled = true
    }
  }, [session, setError])

  const handleDiscover = () =>
    run(async () => {
      setFound(undefined)
      const list = await session.sdk.discoverFederations(
        DiscoverySource.Nostr.new({
          relays: relays.split(/[\s,]+/).filter(Boolean),
        }),
      )
      // Each invite code is a handle into the worker; its text is what the join form takes.
      setFound(
        await Promise.all(
          list.map(async (f) => ({
            id: f.id,
            name: f.name,
            network: f.network,
            modules: f.modules,
            invite: await f.invite.display(),
          })),
        ),
      )
    })

  return (
    <>
      <div className="input-group">
        <label htmlFor="relays">Nostr relays:</label>
        <input
          id="relays"
          type="text"
          value={relays}
          onChange={(e) => setRelays(e.target.value)}
        />
        <button
          type="button"
          onClick={handleDiscover}
          disabled={busy || !relays.trim()}
        >
          {busy ? 'Discovering...' : 'Discover federations'}
        </button>
      </div>
      {found?.length === 0 && (
        <div className="muted">No federation announced itself.</div>
      )}
      {found && found.length > 0 && (
        <div className="wallet-grid">
          {found.map((f) => (
            <div key={f.id} className="wallet-item">
              <button
                type="button"
                className="wallet-button"
                onClick={() => onPick(f.invite)}
                title="Use this invite code"
              >
                <div className="wallet-info">
                  <strong>{f.name ?? 'unnamed'}</strong>
                  <div className="wallet-federation">
                    {short(f.id)}
                    {f.network !== undefined && ` · ${Network[f.network]}`}
                  </div>
                  <div className="wallet-status">{f.modules.join(', ')}</div>
                </div>
              </button>
            </div>
          ))}
        </div>
      )}
      {error && <div className="error">{error}</div>}
    </>
  )
}
