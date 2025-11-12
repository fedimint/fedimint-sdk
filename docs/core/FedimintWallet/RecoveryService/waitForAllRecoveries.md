# Wait for All Recoveries

### `recovery.waitForAllRecoveries()`

Wait for all pending recovery operations to complete.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

try {
  console.log('Waiting for recoveries to complete...')
  await wallet.recovery.waitForAllRecoveries() // [!code focus]
  console.log('All recoveries completed')
} catch (error) {
  console.error('Recovery failed', error)
}
```

## Returns

Returns a `Promise<void>` that resolves when all recovery operations are complete.
