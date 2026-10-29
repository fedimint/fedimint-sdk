import { useState } from 'react'
import { useAction } from '../hooks'
import { useSession } from '../sdk'

/** Reads an invite code without joining anything: `InviteCode.parse`. */
export function ParseInviteCode() {
  const session = useSession()
  const [input, setInput] = useState('')
  const [parsed, setParsed] = useState<{
    federationId: string
    display: string
  }>()
  const { run, busy, error } = useAction()

  const handleParse = () =>
    run(async () => {
      const code = await session.InviteCode.parse(input.trim())
      setParsed({
        federationId: await code.federationId(),
        display: await code.display(),
      })
    })

  return (
    <div className="section">
      <h3>Parse Invite Code</h3>
      <div className="input-group">
        <input
          type="text"
          placeholder="Enter invite code to parse"
          value={input}
          onChange={(e) => {
            setInput(e.target.value)
            setParsed(undefined)
          }}
        />
        <button
          type="button"
          onClick={handleParse}
          disabled={busy || !input.trim()}
        >
          {busy ? 'Parsing...' : 'Parse Invite Code'}
        </button>
      </div>
      {parsed && (
        <div className="success">
          <strong>Parsed Invite Code:</strong>
          <div>
            <strong>Federation ID:</strong> {parsed.federationId}
          </div>
          <details>
            <summary>Canonical form</summary>
            <pre className="invoice-wrap">{parsed.display}</pre>
          </details>
        </div>
      )}
      {error && <div className="error">{error}</div>}
    </div>
  )
}
