# Wait for Recovery

### `recovery.waitForRecovery()`

Wait for all pending recovery operations of a particular wallet to complete.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

try {
  console.log('Waiting for recoveries to complete...')
  await wallet.recovery.waitForRecovery() // [!code focus]
  console.log('All recoveries completed')
} catch (error) {
  console.error('Recovery failed', error)
}
```

## Returns

Returns a `Promise<void>` that resolves when all recovery operations are complete.
