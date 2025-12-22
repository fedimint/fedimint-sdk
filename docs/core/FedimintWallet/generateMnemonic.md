# generateMnemonic

### `generateMnemonic(): Promise<string[]>`

Generates and sets a new mnemonic phrase for the wallet.

This method creates a new BIP39-compatible mnemonic seed phrase that can be used for wallet recovery. The generated mnemonic is automatically set as the wallet's seed phrase and returned for secure storage by the user.

#### Returns

`string[]` - An array of mnemonic words (typically 12 or 24 words).

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

// Generate a new mnemonic phrase
const mnemonic = await director.generateMnemonic() // [!code focus]

console.log('Generated mnemonic:', mnemonic)
// Save this mnemonic securely - it's needed for wallet recovery
// Example: ['word1', 'word2', 'word3', ..., 'word12']
```

#### Related Methods

- [`hasMnemonicSet()`](./hasMnemonicSet.md) - Checks if a mnemonic has been set
- [`getMnemonic()`](./getMnemonic.md) - Retrieves the current mnemonic phrase
- [`setMnemonic(words)`](./setMnemonic.md) - Sets a mnemonic phrase
