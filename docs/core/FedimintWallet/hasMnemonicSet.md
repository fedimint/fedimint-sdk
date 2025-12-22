# hasMnemonicSet

### `hasMnemonicSet(): Promise<boolean>`

Checks if a mnemonic phrase has been set for the wallet.

This method allows you to determine whether a mnemonic seed phrase has been previously configured. It's useful for conditional logic in wallet initialization flows, such as deciding whether to generate a new mnemonic or prompt the user to import an existing one.

#### Returns

`boolean` - Returns `true` if a mnemonic has been set, `false` otherwise.

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

// Check if a mnemonic has been set
const hasMnemonic = await director.hasMnemonicSet() // [!code focus]

if (!hasMnemonic) {
  // Generate a new mnemonic if none exists
  const mnemonic = await director.generateMnemonic()
  console.log('Generated new mnemonic:', mnemonic)
} else {
  // Retrieve the existing mnemonic
  const existingMnemonic = await director.getMnemonic()
  console.log('Existing mnemonic found')
}
```

#### Related Methods

- [`generateMnemonic()`](./generateMnemonic.md) - Generates a new mnemonic phrase
- [`getMnemonic()`](./getMnemonic.md) - Retrieves the current mnemonic phrase
- [`setMnemonic(words)`](./setMnemonic.md) - Sets a mnemonic phrase
