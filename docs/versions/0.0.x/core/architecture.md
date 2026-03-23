# Architecture

The Fedimint Sdk **Core Web** library is a modular and extensible JavaScript library designed to interact with the Fedimint client in a web browser. It provides a high-level API for developers to manage federated ecash wallets, perform operations like minting and spending ecash, and interact with the Lightning Network. The library is structured to promote maintainability, scalability, and ease of use, leveraging modern software development practices.

The **Core Web** library is built around a set of composable services and a communication layer that interacts with a Web Worker running WebAssembly (WASM) code.

<img src="/architecture-diagram.svg" alt="Architecture" />

## [**FedimintWallet**](FedimintWallet/index)

The `FedimintWallet` class serves as the main entry point for the library. It orchestrates the various services and the TransportClient.

[Code](https://github.com/fedimint/fedimint-sdk/blob/main/packages/core/src/FedimintWallet.ts)

## **TransportClient**

The `TransportClient` manages all communication between the main js thread and environment-dependent transport (wasm for web, NativeModule for react-native, etc.).

- Initializes and maintains the Transport instance.
- Handles message passing and response callbacks.
- Provides methods for sending RPC requests and handling streaming responses.

::: info
The `TransportClient` should not be used directly by the end user. Instead, the `FedimintWallet` class should be used to interact with the library.
:::

[Code](https://github.com/fedimint/fedimint-sdk/blob/main/packages/core/src/worker/TransportClient.ts)

## Services

The library is decomposed into several services, each encapsulating a specific domain of functionality. This modular design allows for easier maintenance and testing.

### **FederationService**

Handles federation-related operations such as joining federations, retrieving configurations, and obtaining invite codes.

### **MintService**

Manages operations related to minting and handling ecash notes, including redeeming, reissuing, spending, and validating notes.

### **LightningService**

Facilitates interactions with the Lightning Network, handling invoice creation/payment, gateway management, and event subscriptions.

### **BalanceService**

Manages balance inquiries and subscriptions, allowing users to fetch current wallet balance and subscribe to changes.

### **RecoveryService**

Manages recovery operations, including checking for pending recoveries, waiting for completion, and subscribing to progress updates.

## Utilities

The library includes a configurable logging utility to aid in development and debugging.

### **Logger Utility**

A `Logger` class supports multiple log levels: `DEBUG`, `INFO`, `WARN`, `ERROR`, and `NONE`.

Users can set the desired log level using the `setLogLevel` method.
