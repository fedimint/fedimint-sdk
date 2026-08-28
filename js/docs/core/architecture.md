# Architecture

The Fedimint SDK separates platform and transport lifecycle concerns from wallet
operations. Application code starts with a `WalletDirector`, then obtains a
`FedimintWallet` through `createWallet()`.

<img
  src="/architecture-diagram.svg"
  alt="WalletDirector owns the TransportClient and creates FedimintWallet instances, which expose wallet services"
/>

## WalletDirector

`WalletDirector` is the public creation and configuration entry point. It:

- accepts or configures the platform-specific transport;
- owns and initializes the `TransportClient`;
- creates `FedimintWallet` instances;
- provides utilities that do not require an open wallet, such as parsing,
  federation previews, mnemonic management, and logging configuration.

For browser applications, pass a `WasmWorkerTransport`. Platform packages may
provide a specialized director that configures the appropriate transport.

[Code](https://github.com/fedimint/fedimint-sdk/blob/main/js/shared/core/src/WalletDirector.ts)

## FedimintWallet

`FedimintWallet` is returned by `WalletDirector.createWallet()`. It manages the
open or join lifecycle for an individual wallet client and exposes the wallet's
domain services. It is exported from `@fedimint/core` as a TypeScript type, not as
a directly constructible production value.

[Creating a FedimintWallet](FedimintWallet/createWallet)

[Code](https://github.com/fedimint/fedimint-sdk/blob/main/js/shared/core/src/FedimintWallet.ts)

## TransportClient

`TransportClient` manages communication between JavaScript and the
environment-dependent transport, such as the browser WASM worker or a native
module. It handles initialization, request routing, subscriptions, and transport
errors.

Application code normally does not need to construct or pass a `TransportClient`;
the `WalletDirector` creates it and shares it with the wallets it produces.

[Code](https://github.com/fedimint/fedimint-sdk/blob/main/js/shared/core/src/transport/TransportClient.ts)

## Services

`FedimintWallet` groups operations into focused services:

- `FederationService`: federation configuration, metadata, operations, and
  transactions;
- `MintService`: ecash redemption, spending, parsing, and note queries;
- `LightningService`: invoice creation and payment;
- `BalanceService`: balance queries and subscriptions;
- `RecoveryService`: recovery status and progress;
- `WalletService`: on-chain wallet operations.
