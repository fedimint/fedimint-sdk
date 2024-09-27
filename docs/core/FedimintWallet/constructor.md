# FedimintWallet

Creates a new instance of FedimintWallet.

### `new FedimintWallet(lazy?: boolean)`

This constructor initializes a FedimintWallet instance, which manages communication
with a Web Worker. The Web Worker is responsible for running WebAssembly code that
handles the core Fedimint Client operations.

- (default) When not in **`lazy` mode**, the constructor immediately initializes the
  Web Worker and begins loading the WebAssembly module in the background.

- In **`lazy` mode**, the Web Worker and WebAssembly initialization are deferred until
  the first operation that requires them, reducing initial overhead at the cost
  of a slight delay on the first operation.

::: tip

`lazy` mode is useful for applications where Fedimint Wallet functionality is not needed immediately. This allows you to create a top-level `FedimintWallet` instance without slowing down your initial page load.

You can later initialize the wallet and open it when needed.
:::

#### Parameters

• **lazy**: `boolean` = `false`

If true, delays Web Worker and WebAssembly initialization
until needed. Default is false.

#### Returns

[`FedimintWallet`](constructor.md)

#### Example

```ts
import { FedimintWallet } from '@fedimint/core-web'

// Create a wallet with immediate initialization
const wallet = new FedimintWallet()
wallet.open()

// Create a wallet with lazy initialization
const lazyWallet = new FedimintWallet(true)
// Some time later...
lazyWallet.initialize().then(() => {
  lazyWallet.open()
})
```

#### Defined in

[FedimintWallet.ts:59](https://github.com/fedimint/fedimint-web-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core-web/src/FedimintWallet.ts#L59)
