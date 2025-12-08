# parseOobNotes

### `parseOobNotes(notes: string): Promise<ParsedNoteDetails>`

Parses OOB notes and retrieves their details. It allows you to inspect the contents of OOB notes before redeeming them. This includes the total amount, federation ID, invite code (if present), and note denomination breakdown.

#### Parameters

- `notes` - The OOB notes string to be parsed

#### Returns

`ParsedNoteDetails` - An object containing:

```ts
{
  total_amount: number // The total amount of all notes in millisats
  federation_id_prefix: string // 4-byte hex string identifying the federation
  federation_id?: string // Full 32-byte hex string (if invite is present)
  invite_code?: string // Bech32 encoded invite code starting with "fed1" (if present)
  note_counts: Record<string, number> // Map of denomination amounts (as strings) to their counts
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

console.log(parsedNotes.total_amount, parsedNotes.federation_id_prefix)
console.log(parsedNotes.note_counts) // e.g., { "1000": 5, "5000": 2 }
```
