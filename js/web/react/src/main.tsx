import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App.tsx'
import './index.css'
import { FedimintWalletProvider, setupFedimintWallet } from '../lib/contexts'

// Setup singleton client
setupFedimintWallet({
  lazy: false,
  debug: true,
})

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {/* Wrap the app in the FedimintWalletProvider */}

    <FedimintWalletProvider>
      <App />
    </FedimintWalletProvider>
  </React.StrictMode>,
)
