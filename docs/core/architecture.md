# Architecture

The Fedimint SDK separates platform transport and runtime lifecycle concerns from high-level wallet operations. Application code starts with a `WalletDirector`, which configures the underlying transport and creates a `FedimintWallet` via `createWallet()`.

<img
  src="/architecture-diagram.svg"
  alt="WalletDirector owns the TransportClient and creates FedimintWallet instances, which expose wallet services"
/>

## Core Abstractions

### 1. WalletDirector

`WalletDirector` is the public creation and platform configuration entry point. It:

- Accepts and initializes the platform-specific transport (`WasmWorkerTransport` for Web or `ReactNativeTransport` for Mobile);
- Owns and initializes the `TransportClient`;
- Produces `FedimintWallet` instances;
- Provides offline utilities (mnemonic generation, invite code parsing, invoice validation, and logging configuration) without requiring an open wallet.

```ts
// Web Browser setup (WASM Worker)
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'
const director = new WalletDirector(new WasmWorkerTransport())

// React Native setup (Native UniFFI FFI)
import WalletDirector from '@fedimint/react-native'
const director = new WalletDirector(databasePath)
```

### 2. TransportClient & Platform Transports

`TransportClient` manages JSON-RPC communication between JavaScript and the underlying Rust client runtime. It handles request serialization, streaming subscription multiplexing, and error normalization.

The SDK provides two official transport implementations:

- **`WasmWorkerTransport` (`@fedimint/transport-web`)**:
  - Connects to `fedimint-client-wasm` running in a dedicated browser Web Worker.
  - Keeps heavy Chaumian crypto and networking off the browser main thread.
  - Persists wallet databases to the Origin Private File System (OPFS) or memory storage.

- **`ReactNativeTransport` (`@fedimint/react-native`)**:
  - Connects to `fedimint-client-uniffi` compiled directly into native iOS (`.xcframework`) and Android (`.aar`) binaries.
  - Uses UniFFI C/JNI boundaries with persistent asynchronous callbacks for streaming subscriptions.
  - Persists wallet database files to standard mobile app storage paths.

### 3. FedimintWallet

`FedimintWallet` is returned by `WalletDirector.createWallet()`. It manages the active federation connection (`open()` or `joinFederation()`) and exposes focused domain services.

## Domain Services

`FedimintWallet` groups functionality into modular services:

- **`FederationService` (`wallet.federation`)**: Federation configuration, consensus metadata, gateway lists, and operation history.
- **`MintService` (`wallet.mint`)**: Ecash note spending, out-of-band note reissue, and note validation.
- **`LightningService` (`wallet.lightning`)**: Bolt11 invoice generation, invoice decoding, and outgoing payments.
- **`BalanceService` (`wallet.balance`)**: Balance queries and real-time push update subscriptions.
- **`RecoveryService` (`wallet.recovery`)**: Mnemonic recovery progress, wallet restoration, and backup status.
- **`WalletService` (`wallet.wallet`)**: On-chain Bitcoin peg-in address generation and safe deposit monitoring.
