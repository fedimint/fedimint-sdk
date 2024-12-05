# Spend Ecash

### `mint.parseNotes(oobNotes: string)`

Parses an ecash note string without redeeming it. Use [`redeemEcash()`](./redeemEcash) to redeem the notes.

```ts twoslash
// @esModuleInterop
import { FedimintWallet } from '@fedimint/core-web'

const wallet = new FedimintWallet()
wallet.open()

const longEcashNoteString = '01A...'

const valueMsats = await wallet.mint.parseNotes(longEcashNoteString) // [!code focus]

// later...

await wallet.mint.redeemEcash(longEcashNoteString)
```
