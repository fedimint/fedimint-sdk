# Overview

The `@fedimint/core` package provides a javascript interface for running a fedimint client in the browser.

<div class="tip custom-block" style="padding-top: 8px">

Just want to try it out? Skip to the [Quickstart](./getting-started).

</div>

The `@fedimint/core` package contains a robust, fault-tolerant fedimint client via a [wasm](https://webassembly.org/) module that runs in a web worker. This wasm module is compiled from the rust-based [fedimint client](https://github.com/fedimint/fedimint/tree/master/fedimint-client-wasm).

## Key Features:

- 🚀 **WebAssembly-powered Client**: Exposes the robust, fault-tolerant fedimint-client (built in Rust) via WebAssembly. Lazy-Loads within a web worker for performance.

- 💰 **Ecash Payments**: Includes support for joining federations, sending/receiving ecash, and managing balances.

- ⚡ **Lightning Payments**: Ships with zero-setup Lightning Network payments.

- 🛠️ **State Management**: Handles the complex state management and storage challenges for browser wallets.

- 🤫 **Privacy Included**: Offers a privacy-centric wallet by default.

- ⚙️ **Framework Agnostic**: Designed as a "core" library compatible with vanilla JavaScript, laying the groundwork for future framework-specific packages.

## Mission

Our goal is to provide the **best possible developer experience** for building with bitcoin, lowering the barrier to entry for creating safe, robust, privacy-centric applications.

Looking ahead, we plan to expand this SDK with framework-specific libraries, starting with [React](https://react.dev/).
