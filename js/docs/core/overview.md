# Overview

The `@fedimint/core` package provides a multi-platform TypeScript and JavaScript interface for running Fedimint clients in web browsers and mobile applications.

<div class="tip custom-block" style="padding-top: 8px">

Just want to try it out? Skip to the [Quickstart](./getting-started).

</div>

`@fedimint/core` provides high-level wallet services, state management, and lifecycle coordination on top of the underlying Rust-based [fedimint client](https://github.com/fedimint/fedimint). It communicates with the Rust engine through platform-specific transport adapters:

1. **Browser WebAssembly (`@fedimint/transport-web`)**: Runs the `fedimint-client-wasm` engine lazily inside a dedicated Web Worker, persisting data using Origin Private File System (OPFS) or IndexedDB.
2. **Native Mobile FFI (`@fedimint/react-native`)**: Connects React Native and Expo applications to native Rust binaries compiled via in-tree `fedimint-client-uniffi` (supporting iOS `.xcframework` and Android `.aar`).

Applications configure the platform integration through a `WalletDirector` and obtain a `FedimintWallet` by calling `director.createWallet()`. The wallet is not constructed directly. This keeps transport initialization in the director while the returned wallet focuses on opening or joining a federation and providing domain services.

## Key Features:

- 🚀 **Multi-Platform Execution**: Run natively in iOS & Android apps via UniFFI FFI bindings, or in web browsers via WebAssembly.
- 💰 **Ecash Payments**: First-class support for joining federations, out-of-band ecash note reissue, and note spending.
- ⚡ **Zero-Setup Lightning**: Send and receive instant Lightning Network payments via federation Lightning gateways.
- 🛠️ **Robust State Management**: Handles asynchronous persistence, database locking, and background balance streams across web and mobile.
- 🤫 **Privacy by Default**: Chaumian blinded token mints ensure sender and recipient financial privacy.
- ⚙️ **Framework Agnostic**: Core TypeScript library compatible with vanilla JS, React, Next.js, React Native, and Expo.

## Architecture at a Glance

```
┌─────────────────────────────────────────────────────────────┐
│                       @fedimint/core                        │
│            (WalletDirector & FedimintWallet)                │
└──────────────┬───────────────────────────────┬──────────────┘
               │                               │
               ▼ (WasmWorkerTransport)         ▼ (ReactNativeTransport)
┌─────────────────────────────┐ ┌─────────────────────────────┐
│  @fedimint/transport-web    │ │   @fedimint/react-native    │
│    (Browser Web Worker)     │ │  (UniFFI Native Bindings)   │
└──────────────┬──────────────┘ └──────────────┬──────────────┘
               ▼                               ▼
┌─────────────────────────────┐ ┌─────────────────────────────┐
│    fedimint-client-wasm     │ │   fedimint-client-uniffi    │
│     (WebAssembly / OPFS)    │ │   (iOS XCFramework / AAR)   │
└─────────────────────────────┘ └─────────────────────────────┘
```

## Mission

Our goal is to provide the **best possible developer experience** for building with bitcoin and ecash, lowering the barrier to entry for creating safe, robust, privacy-centric applications across web and mobile platforms.
