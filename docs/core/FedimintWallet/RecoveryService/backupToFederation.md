# Backup to Federation

### `recovery.backupToFederation(metadata?: JSONValue)`

Backup the wallet state to the federation with optional metadata.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

// Backup without metadata
try {
  await wallet.recovery.backupToFederation() // [!code focus]
  console.log('Backup successful')
} catch (error) {
  console.error('Failed to backup', error)
}

// Backup with metadata
try {
  await wallet.recovery.backupToFederation({
    // [!code focus]
    timestamp: Date.now(), // [!code focus]
    note: 'My backup', // [!code focus]
  }) // [!code focus]
  console.log('Backup with metadata successful')
} catch (error) {
  console.error('Failed to backup', error)
}
```

## Parameters

- `metadata` (optional): A JSON value containing custom metadata to store with the backup. Defaults to an empty object if not provided.

## Returns

Returns a `Promise<void>` that resolves when the backup is complete.
