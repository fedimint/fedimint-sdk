# Has Pending Recoveries

### `recovery.hasPendingRecoveries()`

Check if there are any pending recovery operations.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

try {
  const hasPending = await wallet.recovery.hasPendingRecoveries() // [!code focus]
  if (hasPending) {
    console.log('Recovery operations in progress')
  } else {
    console.log('No pending recoveries')
  }
} catch (error) {
  console.error('Failed to check recovery status', error)
}
```

## Returns

Returns a `Promise<boolean>` that resolves to `true` if there are pending recoveries, `false` otherwise.
