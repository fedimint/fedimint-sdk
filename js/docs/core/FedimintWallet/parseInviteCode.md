# parseInviteCode

### `parseInviteCode(inviteCode: string): Promise<ParsedInviteCode>`

Parses an invite code and extracts its components (`federation_id` and `url`)

This is a utility method that can be called immediately after creating a `WalletDirector` instance.

#### Returns

`ParsedInviteCode` - An object with the following structure:

```ts
{
  federation_id: string
  url: string
}
```

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const walletDirector = new WalletDirector(new WasmWorkerTransport())

// No need to open wallet or join federation
const result = await walletDirector.parseInviteCode('fed11.......') // [!code focus]

console.log(result.federation_id) // '15db8cb4f1ec...'
console.log(result.url) // 'wss://......'
```
