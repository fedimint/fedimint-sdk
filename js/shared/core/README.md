# @fedimint/core

A platform-agnostic TypeScript SDK for integrating Fedimint ecash and Lightning payments into web, mobile, and native applications.

### ⚠️ Early Software Notice

APIs may evolve. Please report issues and feedback on the [Fedimint SDK GitHub](https://github.com/fedimint/fedimint-sdk/issues).

---

## 🚀 Overview

`@fedimint/core` contains the core wallet services, state management, and high-level orchestration logic. It connects to the underlying Rust Fedimint client through platform-specific transport implementations:

- **Web (Browsers)**: Connects via `@fedimint/transport-web` (WebAssembly running inside a dedicated Web Worker).
- **Mobile (React Native / Expo)**: Connects via `@fedimint/react-native` (native FFI bridge powered by in-tree `fedimint-client-uniffi`).

---

## 📦 Multi-Platform Quickstart

### 1. Browser Applications (WebAssembly)

Install `@fedimint/core` alongside `@fedimint/transport-web`:

```bash
npm install @fedimint/core @fedimint/transport-web
```

```typescript
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

// 1. Initialize director with Web Worker transport
const director = new WalletDirector(new WasmWorkerTransport())

// 2. Create wallet instance and open or join federation
const wallet = await director.createWallet()
await wallet.joinFederation('fed11...')

// 3. Use wallet services
const balance = await wallet.balance.getBalance()
```

### 2. React Native & Expo Applications (Native FFI)

Install `@fedimint/react-native` (which bundles `@fedimint/core` and prebuilt native FFI binaries):

```bash
# Bare React Native
npm install @fedimint/react-native react-native-fs

# Expo Managed
npx expo install @fedimint/react-native expo-file-system
```

```typescript
import WalletDirector from '@fedimint/react-native'
import RNFS from 'react-native-fs'

const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`
const director = new WalletDirector(dbPath)

const wallet = await director.createWallet()
await wallet.joinFederation('fed11...')
```

---

## 🛠️ Architecture & Services

The SDK exposes domain services through `FedimintWallet`:

- **`wallet.balance`**: Real-time balance queries and push subscriptions.
- **`wallet.mint`**: Ecash out-of-band note reissue and spending.
- **`wallet.lightning`**: Instant Lightning invoice creation and payment.
- **`wallet.federation`**: Consensus metadata, gateway discovery, and operation logs.
- **`wallet.wallet`**: On-chain Bitcoin peg-in address generation.

---

## 📚 Documentation & Examples

- [Official Documentation](https://sdk.fedimint.org/core/getting-started.html)
- [Vite + React Example](https://sdk.fedimint.org/examples/vite-react)
- [Expo Mobile Example](https://github.com/fedimint/fedimint-sdk/tree/main/examples/expo-app)
- [Bitcoin Mints Directory](https://bitcoinmints.com/?tab=mints&showFedimint=true) - Public federations with invite codes
