import { Network, type Capabilities } from '@fedimint/web-sdk'
import { useEffect, useState } from 'react'
import { errorText, msat } from '../format'
import { useAction, useBalance } from '../hooks'
import { useFederation, useSdk, useSession } from '../sdk'

const modules = (capabilities: Capabilities) =>
  Object.entries(capabilities)
    .filter(([, available]) => available)
    .map(([name]) => name)
    .join(', ') || 'none'

/**
 * The federation in use and its balance, kept current: `id`, `name`, `network`,
 * `capabilities`, `inviteCode`, `balance`, `balanceUpdates`, `sdk.closeFederation`,
 * `sdk.forgetFederation`.
 */
export function WalletStatus() {
  const session = useSession()
  const federation = useFederation()
  const { federationId, deselect, federationsChanged } = useSdk()
  const balance = useBalance(federation)
  const [info, setInfo] = useState<{
    name?: string
    network: string
    modules: string
    invite: string
  }>()
  const { run, busy, error, setError } = useAction()

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const [name, network, capabilities, invite] = await Promise.all([
          federation.name(),
          federation.network(),
          federation.capabilities(),
          federation.inviteCode(),
        ])
        const display = await invite.display()
        if (!cancelled)
          setInfo({
            name,
            network: Network[network],
            modules: modules(capabilities),
            invite: display,
          })
      } catch (e) {
        if (!cancelled) setError(errorText(e))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [federation, setError])

  // The SDK call comes first and the federation is deselected only once it succeeded: this
  // section unmounts with the deselection, and a refusal (`BalanceNotEmpty` on a forget, for
  // one) has to be shown here.
  const handleClose = () =>
    run(async () => {
      await session.sdk.closeFederation(federationId!)
      deselect(federationId!)
      federationsChanged()
    })

  const handleForget = () =>
    run(async () => {
      // Names the id the call below takes, whatever has loaded by now.
      const target = info?.name
        ? `${info.name} (${federationId})`
        : federationId
      if (!confirm(`Forget ${target} and erase its data?`)) return
      await session.sdk.forgetFederation(federationId!)
      deselect(federationId!)
      federationsChanged()
    })

  return (
    <div className="section">
      <h3>Wallet Status</h3>
      <div className="row">
        <strong>Balance:</strong>
        <div className="balance">{msat(balance)}</div>
      </div>
      <div className="row">
        <strong>Federation:</strong>
        <div>{info?.name ?? '…'}</div>
      </div>
      <div className="row">
        <strong>Federation ID:</strong>
        <div style={{ wordBreak: 'break-all' }}>{federationId}</div>
      </div>
      <div className="row">
        <strong>Network:</strong>
        <div>{info?.network ?? '…'}</div>
      </div>
      <div className="row">
        <strong>Modules:</strong>
        <div>{info?.modules ?? '…'}</div>
      </div>
      {info && (
        <details>
          <summary>Invite code</summary>
          <pre className="invoice-wrap">{info.invite}</pre>
        </details>
      )}
      <div className="button-group">
        <button type="button" onClick={handleClose} disabled={busy}>
          Close Federation
        </button>
        <button type="button" onClick={handleForget} disabled={busy}>
          Forget Federation
        </button>
      </div>
      {error && <div className="error">{error}</div>}
    </div>
  )
}
