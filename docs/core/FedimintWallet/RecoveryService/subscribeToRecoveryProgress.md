# Subscribe to Recovery Progress

### `recovery.subscribeToRecoveryProgress(onSuccess, onError)`

Subscribe to recovery progress updates for all modules.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const unsubscribe = wallet.recovery.subscribeToRecoveryProgress(
  // [!code focus]
  (progress) => {
    // [!code focus]
    console.log('Module:', progress.module_id) // [!code focus]
    console.log('Progress:', progress.progress) // [!code focus]
  }, // [!code focus]
  (error) => {
    // [!code focus]
    // [!code focus]
    console.error('Recovery error:', error) // [!code focus]
  }, // [!code focus]
) // [!code focus]

// Later, to stop listening for updates
unsubscribe()
```

## Parameters

- `onSuccess`: Callback function invoked with progress updates. Receives an object containing:
  - `module_id`: The module identifier (number)
  - `progress`: The progress data (JSONValue)
- `onError`: Callback function invoked when an error occurs. Receives an error string.

## Returns

Returns a function that can be called to unsubscribe from progress updates.
