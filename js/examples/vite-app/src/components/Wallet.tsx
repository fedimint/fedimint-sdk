import { useState } from 'react'
import { useAction } from '../hooks'
import { useSdk } from '../sdk'

type Mode = 'create' | 'recover'

interface Choice {
  title: string
  detail: string
  storage: string
}

/**
 * The two ways in, and the storage name each suggests. Nothing tells a saved name from an unused
 * one before opening it, so the first both reopens and creates.
 */
const modes: Record<Mode, Choice> = {
  create: {
    title: 'Open or create a wallet',
    detail:
      'Reopens the wallet saved under the name, or creates one with a new seed.',
    storage: 'vite-example',
  },
  recover: {
    title: 'Recover a wallet',
    detail:
      'Restores a wallet from its seed words, under a name not yet in use.',
    storage: 'vite-recovered',
  },
}

/**
 * Opens the SDK over a named store, which persists across reloads, either with the seed saved
 * there or from seed words, and shows the seed: `openSdk`, `sdk.exportMnemonic`, `session.close`.
 */
export function Wallet() {
  const { session, open, close } = useSdk()
  const [mode, setMode] = useState<Mode>('create')
  const [storage, setStorage] = useState(modes.create.storage)
  const [words, setWords] = useState('')
  const [mnemonic, setMnemonic] = useState('')
  const [showMnemonic, setShowMnemonic] = useState(false)
  const [message, setMessage] = useState('')
  const { run, busy, error } = useAction()

  const choose = (next: Mode) => {
    setMode(next)
    setStorage(modes[next].storage)
    setMessage('')
  }

  const handleOpen = async (e: React.FormEvent) => {
    e.preventDefault()
    setMessage('')
    const name = storage.trim()
    const started = performance.now()
    await run(async () => {
      const recovering = mode === 'recover'
      await open(
        name,
        recovering ? words.trim().split(/\s+/).filter(Boolean) : undefined,
      )
      const took = Math.round(performance.now() - started)
      setWords('')
      setMessage(
        recovering
          ? `Recovered "${name}" in ${took} ms: recover each federation it used, below`
          : `Opened "${name}" in ${took} ms`,
      )
    })
  }

  const handleClose = () =>
    run(async () => {
      await close()
      setMnemonic('')
      setMessage('Closed the wallet')
    })

  const handleShow = () =>
    run(async () => {
      const exported = await session!.sdk.exportMnemonic()
      setMnemonic((await exported.words()).join(' '))
      setShowMnemonic(false)
    })

  return (
    <div className="section">
      <h3>Wallet</h3>
      {session ? (
        <>
          <div className="row">
            <strong>Storage:</strong> {storage.trim()}
          </div>
          <div className="button-group">
            <button type="button" onClick={handleShow} disabled={busy}>
              Show Seed
            </button>
            <button type="button" onClick={handleClose} disabled={busy}>
              Close Wallet
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="wallet-grid">
            {(Object.keys(modes) as Mode[]).map((m) => (
              <div
                key={m}
                className={`wallet-item ${m === mode ? 'active' : ''}`}
              >
                <button
                  type="button"
                  className="wallet-button"
                  disabled={busy}
                  onClick={() => choose(m)}
                >
                  <div className="wallet-info">
                    <strong>{modes[m].title}</strong>
                    <div className="wallet-federation">{modes[m].detail}</div>
                  </div>
                </button>
              </div>
            ))}
          </div>
          <form onSubmit={handleOpen}>
            <div className="input-group">
              <label htmlFor="storage">Storage name:</label>
              <input
                id="storage"
                required
                value={storage}
                onChange={(e) => setStorage(e.target.value)}
              />
            </div>
            {mode === 'recover' && (
              <div className="input-group">
                <label htmlFor="words">Seed words:</label>
                <textarea
                  id="words"
                  required
                  rows={2}
                  placeholder="The wallet's seed words, separated by spaces"
                  value={words}
                  onChange={(e) => setWords(e.target.value)}
                />
              </div>
            )}
            <div className="button-group">
              <button
                type="submit"
                disabled={
                  busy ||
                  !storage.trim() ||
                  (mode === 'recover' && !words.trim())
                }
              >
                {busy
                  ? 'Opening...'
                  : mode === 'recover'
                    ? 'Recover Wallet'
                    : 'Open Wallet'}
              </button>
            </div>
          </form>
        </>
      )}
      {mnemonic && (
        <div className="mnemonic-output">
          <span className={showMnemonic ? '' : 'blurred'}>{mnemonic}</span>
          <button
            type="button"
            className="btn-small"
            onClick={() => setShowMnemonic(!showMnemonic)}
          >
            {showMnemonic ? 'Hide' : 'Show'}
          </button>
          <button
            type="button"
            className="btn-small"
            disabled={!showMnemonic}
            onClick={() => navigator.clipboard.writeText(mnemonic)}
          >
            Copy
          </button>
        </div>
      )}
      {message && <div className="success">{message}</div>}
      {error && <div className="error">{error}</div>}
    </div>
  )
}
