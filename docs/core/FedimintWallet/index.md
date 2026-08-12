# Wallet API overview

`WalletDirector` is the public entry point for configuring the platform transport
and creating wallets. A `FedimintWallet` is the wallet operations facade returned
by `WalletDirector.createWallet()`; application code should not construct it
directly.

::: info
See [Getting Started](../getting-started) for the complete browser setup.
:::

## Creating a wallet

```ts twoslash
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()
```

The returned wallet is not open yet. Use one of the following lifecycle paths:

```ts twoslash
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

// Open client state that already exists.
await wallet.open()

// Or, on an unopened wallet, join a federation.
// await wallet.joinFederation('fed11...')
```

See [Creating a FedimintWallet](createWallet) for the creation and initialization
contract.

## Responsibilities

### WalletDirector

The director owns the `TransportClient` and its initialization. It is also the
home for operations that do not require an open wallet, including:

- configuring logging;
- previewing a federation;
- parsing invite codes, Lightning invoices, and OOB notes;
- generating, reading, and setting the mnemonic;
- creating `FedimintWallet` instances.

### FedimintWallet

The wallet represents an individual client lifecycle. It provides `open()`,
`joinFederation()`, `isOpen()`, and access to these services:

- `balance`: balance queries and subscriptions;
- `mint`: ecash redemption and spending;
- `lightning`: Lightning invoice creation and payment;
- `federation`: federation configuration and operation history;
- `recovery`: recovery status and progress;
- `wallet`: on-chain wallet operations.

When `cleanup()` is called, discard that wallet instance. Do not construct a
replacement with `new FedimintWallet()`; obtain one through a `WalletDirector`.

```mermaid
flowchart LR
  app["Application"] --> director["WalletDirector"]
  director -->|"owns"| client["TransportClient"]
  director -->|"createWallet()"| wallet["FedimintWallet"]
  wallet --> services["Wallet services"]
  services -->|"use shared client"| client
  client --> transport["Platform transport"]
```
