# cleanup

### `cleanup()`

Cleans up browser resources associated with the wallet. This should be called when the wallet is no longer needed.

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

// If we want to use the wallet again, we can call open() again
await wallet.open()
```
