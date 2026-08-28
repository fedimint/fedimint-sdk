<p align="center">
  <img src="../../docs/public/icon.png" alt="Fedimint Logo" width="300" />
  <!-- Removes the border below the header tag -->
  <div id="toc"><ul align="center" style="list-style: none;"><summary>
    <h1><b>@fedimint/react</b></h1>
    <p>Helpful React hooks for building with the Fedimint Sdk.</p>
  </summary></ul></div>

# @fedimint/react

## Install

```bash
pnpm install @fedimint/core @fedimint/react
```

## Usage

```tsx
import { FedimintWalletProvider, setupFedimintWallet } from '@fedimint/react'

setupFedimintWallet({
  lazy: false,
  debug: true,
})

// Wrap your app in the FedimintWalletProvider
<FedimintWalletProvider>
  <App />
</FedimintWalletProvider>


// App.tsx


// Balance

import { useBalance } from '@fedimint/react'

const balance = useBalance()

// Wallet

import { useOpenWallet } from '@fedimint/react'

const {
  walletStatus, // 'open' | 'closed' | 'connecting' | 'failed'
  openWallet, // () => Promise<boolean> - Returns true if wallet was opened successfully.
  joinFederation, // () => Promise<boolean> - Returns true if joined federation successfully.
} = useOpenWallet()

// Receive Lightning

import { useReceiveLightning } from '@fedimint/react'

const {
  generateInvoice, // (amount: number) => Promise<void>
  bolt11, // string
  invoiceStatus, // 'pending' | 'confirmed' | 'failed'
  error, // Error | undefined
} = useReceiveLightning()

// Send Lightning

import { useSendLightning } from '@fedimint/react'

const {
  payInvoice, // (bolt11: string) => Promise<void>
  paymentStatus, // 'pending' | 'confirmed' | 'failed'
  paymentError, // Error | undefined
} = useSendLightning()

```
