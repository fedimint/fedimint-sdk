# setMnemonic

### `setMnemonic(words: string[]): Promise<boolean>`

Sets the mnemonic phrase for the wallet.

This method allows you to restore a wallet from an existing mnemonic seed phrase. Use this when a user wants to import their wallet using a previously generated mnemonic.

#### Parameters

- `words` - An array of mnemonic words (typically 12 or 24 words) representing a valid BIP39 mnemonic phrase

#### Returns

`boolean` - Returns `true` if the mnemonic was set successfully.

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

// Restore wallet from existing mnemonic
const existingMnemonic = [
  'word1',
  'word2',
  'word3',
  // ... remaining words
  'word12',
]

const success = await director.setMnemonic(existingMnemonic) // [!code focus]

if (success) {
  console.log('Mnemonic set successfully')
}
```

#### Related Methods

- [`hasMnemonicSet()`](./hasMnemonicSet.md) - Checks if a mnemonic has been set
- [`generateMnemonic()`](./generateMnemonic.md) - Generates a new mnemonic phrase
- [`getMnemonic()`](./getMnemonic.md) - Retrieves the current mnemonic phrase
