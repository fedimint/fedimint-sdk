import { Network, type SdkSession } from '@fedimint/web-sdk'
import { useState } from 'react'
import { faucet } from '../faucet'
import { json, short } from '../format'
import { useAction } from '../hooks'
import { useSdk, useSession } from '../sdk'

/** What `sdk.preview` answers, as it crosses from the worker. */
type Preview = Awaited<ReturnType<SdkSession['sdk']['preview']>>

/**
 * Joins a federation from an invite code, or recovers one: `InviteCode.parse`, `sdk.preview`,
 * `sdk.join`, `sdk.recover`. The invite code can come from devimint's faucet.
 */
export function JoinFederation() {
  const session = useSession()
  const {
    faucetUrl,
    setFaucetUrl,
    selectFederation,
    federationsChanged,
    fromSeed,
  } = useSdk()
  const [inviteCode, setInviteCode] = useState('')
  // A wallet recovered from its words has funds to find, so recovering is the default there.
  const [recover, setRecover] = useState(fromSeed)
  const [preview, setPreview] = useState<Preview>()
  const [result, setResult] = useState('')
  const { run, busy, error } = useAction()

  const invite = () => session.InviteCode.parse(inviteCode.trim())

  const handleFaucet = () =>
    run(async () => {
      setInviteCode(await faucet.inviteCode(faucetUrl))
      setPreview(undefined)
    })

  const handlePreview = () =>
    run(async () => {
      setPreview(await session.sdk.preview(await invite()))
    })

  const handleJoin = async (e: React.FormEvent) => {
    e.preventDefault()
    setResult('')
    await run(async () => {
      if (recover) {
        // Joins and rescans the federation's history for what this seed owns; sends and
        // receives fail with `Recovering` until the recovery is done.
        const { federation } = await session.sdk.recover(await invite())
        await selectFederation(federation)
        setResult(`Recovering federation ${short(await federation.id())}`)
      } else {
        const federation = await session.sdk.join(await invite())
        await selectFederation(federation)
        setResult(`Joined federation ${short(await federation.id())}`)
      }
      federationsChanged()
    })
  }

  return (
    <div className="section">
      <h3>Join a federation</h3>
      <form onSubmit={handleJoin}>
        <textarea
          placeholder="Invite code (fed1...)"
          required
          rows={3}
          value={inviteCode}
          onChange={(e) => {
            setInviteCode(e.target.value)
            setPreview(undefined)
          }}
        />
        <div className="row">
          <input
            id="recover"
            type="checkbox"
            style={{ width: 'auto' }}
            checked={recover}
            onChange={(e) => setRecover(e.target.checked)}
          />
          <label htmlFor="recover">Recover</label>
        </div>
        <div className="button-group">
          <button
            type="button"
            onClick={handlePreview}
            disabled={busy || !inviteCode.trim()}
          >
            Preview Federation
          </button>
          <button type="submit" disabled={busy || !inviteCode.trim()}>
            {busy ? 'Working...' : recover ? 'Recover' : 'Join Federation'}
          </button>
        </div>
      </form>
      <div className="input-group">
        <label htmlFor="faucet">devimint faucet:</label>
        <input
          id="faucet"
          type="url"
          value={faucetUrl}
          onChange={(e) => setFaucetUrl(e.target.value)}
        />
        <button type="button" onClick={handleFaucet} disabled={busy}>
          Get an invite code
        </button>
      </div>

      {preview && (
        <div className="preview-result">
          <h4>Federation Preview</h4>
          <div className="preview-info">
            <div>
              <strong>Federation ID:</strong> {preview.id}
            </div>
            <div>
              <strong>Name:</strong> {preview.name ?? 'unnamed'}
            </div>
            <div>
              <strong>Network:</strong> {Network[preview.network]}
            </div>
            <div>
              <strong>Guardians:</strong> {preview.guardians}
            </div>
            <div>
              <strong>Modules:</strong> {preview.modules.join(', ')}
            </div>
            <details>
              <summary>Meta</summary>
              <pre>{json(preview.meta)}</pre>
            </details>
          </div>
        </div>
      )}

      {result && <div className="success">{result}</div>}
      {error && <div className="error">{error}</div>}
    </div>
  )
}
