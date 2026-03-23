# getMnemonic

### `getMnemonic(): Promise<string[]>`

Retrieves the current mnemonic phrase from the wallet.

This method returns the wallet's existing mnemonic seed phrase. Use this when you need to display the mnemonic to the user for backup purposes or to verify it has been set correctly.

#### Returns

`string[]` - An array of mnemonic words representing the wallet's seed phrase.

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

// Retrieve the current mnemonic
const mnemonic = await director.getMnemonic() // [!code focus]

console.log('Current mnemonic:', mnemonic)
// Display to user for backup purposes
```

#### Related Methods

- [`hasMnemonicSet()`](./hasMnemonicSet.md) - Checks if a mnemonic has been set
- [`generateMnemonic()`](./generateMnemonic.md) - Generates a new mnemonic phrase
- [`setMnemonic(words)`](./setMnemonic.md) - Sets a mnemonic phrase
