# new FedimintWallet()

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

```ts twoslash
// import { FedimintWallet } from '@fedimint/core'

// // Create a wallet with immediate initialization // [!code focus]
// const wallet = new FedimintWallet() // wasm gets initialized here // [!code focus]
// wallet.open()

// // Create a wallet with lazy initialization // [!code focus]
// const lazyWallet = new FedimintWallet(true) // lazy = true // [!code focus]
// // Some time later...
// lazyWallet.open() // wasm gets initialized here
```

#### Defined in

[FedimintWallet.ts:59](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L59)
