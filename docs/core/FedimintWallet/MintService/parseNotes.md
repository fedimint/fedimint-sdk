# Spend Ecash

### `mint.parseNotes(oobNotes: string)`

Parses an ecash note string without redeeming it. Use [`redeemEcash()`](./redeemEcash) to redeem the notes.

> **WARNING**: This function will throw an error if the wallet is currently recovering.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const longEcashNoteString = '01A...'

const valueMsats = await wallet.mint.parseNotes(longEcashNoteString) // [!code focus]

// later...

await wallet.mint.redeemEcash(longEcashNoteString)
```
