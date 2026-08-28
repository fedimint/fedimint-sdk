# parseOobNotes

### `parseOobNotes(notes: string): Promise<ParsedNoteDetails>`

Parses OOB notes and retrieves their details. It allows you to inspect the contents of OOB notes before redeeming them. This includes the total amount, federation ID, invite code (if present), and note denomination breakdown.

#### Parameters

- `notes` - The OOB notes string to be parsed

#### Returns

`ParsedNoteDetails` - An object containing:

```ts
{
  // The total amount of all notes in millisats
  total_amount: number
  // 4-byte hex string identifying the federation
  federation_id_prefix: string
  // Full 32-byte hex string (if invite is present)
  federation_id?: string
  // Bech32 encoded invite code starting with "fed1" (if present)
  invite_code?: string
  // Map of denomination amounts (as strings) to their counts
  note_counts: Record<string, number>
}
```

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

const notes = '...OOB notes string...'
const parsedNotes = await director.parseOobNotes(notes) // [!code focus]

console.log(
  parsedNotes.total_amount,
  parsedNotes.federation_id_prefix,
  parsedNotes.federation_id,
  parsedNotes.invite_code,
)
console.log(parsedNotes.note_counts) // e.g., { "1000": 5, "5000": 2 }
```

### Redeeming from Unknown Federation

If you receive an OOB note from a federation you might not be a member of, you can use `parseOobNotes` to extract the invite code and join before redeeming.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const notes = '...OOB notes string...'
const parsed = await director.parseOobNotes(notes)

if (parsed.invite_code) {
  console.log('Joining new federation...')
  await wallet.joinFederation(parsed.invite_code)
} else {
  throw new Error('Cannot join federation: No invite code in notes')
}

// Now we can safely redeem
await wallet.mint.redeemEcash(notes)
```
