import React from 'react'
import ReactDOM from 'react-dom/client'
import { clearClientStorage } from '@fedimint/transport-web'
import './index.css'

// ── React Error Boundary ──────────────────────────────────────────────
class AppErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props)
    this.state = { error: null }
  }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error('Unhandled app error:', error, info)
  }

  handleWipe = async () => {
    await clearClientStorage()
    window.location.reload()
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            color: '#ff6b6b',
            padding: '2rem',
            fontFamily: 'sans-serif',
          }}
        >
          <h2>Something went wrong</h2>
          <p>{this.state.error.message}</p>
          <button
            style={{
              padding: '10px 20px',
              background: '#ff4444',
              color: 'white',
              border: 'none',
              borderRadius: '4px',
              cursor: 'pointer',
              marginTop: '1rem',
            }}
            onClick={this.handleWipe}
          >
            Wipe Data &amp; Reload
          </button>
        </div>
      )
    }
    return this.props.children
  }
}

// ── Bootstrap ─────────────────────────────────────────────────────────
const MAX_WIPE_RETRIES = 3

const init = async () => {
  // Check if we need to wipe OPFS db (e.g. user requested a hard reset).
  // This must happen BEFORE importing the wallet, because WasmWorkerTransport
  // loads the worker which locks the DB file.
  const wipeCount = Number(localStorage.getItem('wipeRetryCount') ?? '0')

  if (localStorage.getItem('pendingWipe') === 'true') {
    localStorage.removeItem('pendingWipe')

    if (wipeCount >= MAX_WIPE_RETRIES) {
      // Prevent infinite reload loop — give up after N attempts.
      localStorage.removeItem('wipeRetryCount')
      console.error(
        `Storage wipe failed after ${MAX_WIPE_RETRIES} retries. Skipping.`,
      )
    } else {
      localStorage.setItem('wipeRetryCount', String(wipeCount + 1))
      await clearClientStorage()
      localStorage.removeItem('wipeRetryCount')
    }
  }

  // Now dynamically import the wallet and React app
  const App = (await import('./App')).default

  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
      <AppErrorBoundary>
        <App />
      </AppErrorBoundary>
    </React.StrictMode>,
  )
}

init().catch((err) => {
  console.error('Failed to initialize App:', err)
  // Last-resort fallback only if React itself failed to mount.
  // This is intentionally raw DOM because React is unavailable.
  const root = document.getElementById('root')
  if (root) {
    const container = document.createElement('div')
    container.style.color = '#ff6b6b'
    container.style.padding = '2rem'
    container.style.fontFamily = 'sans-serif'
    
    const h2 = document.createElement('h2')
    h2.textContent = 'Failed to initialize app'
    container.appendChild(h2)

    const pErr = document.createElement('p')
    pErr.textContent = err.message
    container.appendChild(pErr)

    const pHelp = document.createElement('p')
    pHelp.style.color = '#888'
    pHelp.style.fontSize = '0.85em'
    pHelp.textContent = "If this persists, clear your browser's site data for localhost."
    container.appendChild(pHelp)

    root.appendChild(container)
  }
})
