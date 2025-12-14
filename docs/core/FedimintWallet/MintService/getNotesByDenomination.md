# GetNotesByDenomination

### `mint.getNotesByDenomination()`

Gives count of ecash notes by denomination present in the wallet.

> **WARNING**: This function will throw an error if the wallet is currently recovering.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const notes = await wallet.mint.getNotesByDenomination() // [!code focus]
console.log('Notes are: ', notes) // {1: 2, 2: 1, 8: 2}
```
