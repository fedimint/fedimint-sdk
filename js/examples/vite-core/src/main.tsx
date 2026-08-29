import React from 'react'
import ReactDOM from 'react-dom/client'
import { clearClientStorage } from '@fedimint/core'
import './index.css'

const init = async () => {
  // Check if we need to wipe OPFS db (e.g. user requested a hard reset)
  // This must happen BEFORE importing the wallet, because WasmWorkerTransport
  // loads the worker which locks the DB file.
  if (localStorage.getItem('pendingWipe') === 'true') {
    localStorage.removeItem('pendingWipe')
    await clearClientStorage()
  }

  // Now dynamically import the wallet and React app
  const App = (await import('./App')).default

  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  )
}

init()
