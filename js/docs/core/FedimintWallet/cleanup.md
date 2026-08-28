# cleanup

### `cleanup()`

Cleans up transport resources associated with the wallet. This should be called
when the wallet is no longer needed.

Cleanup is terminal for the entire `WalletDirector`. It shuts down the
director-owned transport client and invalidates every `FedimintWallet` created by
that director. Call it only after all of those wallets are no longer in use.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()
// ... use the wallet

// Once we're no longer using the wallet, // [!code focus]
// we can call cleanup to free up resources // [!code focus]
await wallet.cleanup() // [!code focus]
```

After cleanup, discard the director and every wallet created by it. To create
another wallet, construct a fresh platform transport and a new `WalletDirector`,
then call `createWallet()` on the new director.
