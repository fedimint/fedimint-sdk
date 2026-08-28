# parseBolt11Invoice

### `parseBolt11Invoice(invoiceStr: string): Promise<ParsedBolt11Invoice>`

Parses a BOLT11 invoice and extracts its components (`amount`, `expiry`, and `memo`)

This is a utility method that can be called immediately after creating a `WalletDirector` instance.

#### Returns

`ParsedBolt11Invoice` - An object with the following structure:

```ts
{
  amount: number // Amount in satoshis (sats)
  expiry: number // Expiry time in seconds
  memo: string // Invoice description/memo
}
```

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

// No need to open wallet or join federation
const result = await director.parseBolt11Invoice('lnbc1....') // [!code focus]

console.log(result.amount) // 1000 (sats)
console.log(result.expiry) // 3600 (seconds)
console.log(result.memo) // 'Payment for services'
```

::: tip Amount Units
The `amount` field is returned in **satoshis (sats)**, not millisatoshis.

- 1 sat = 1,000 msats
- 1 BTC = 100,000,000 sats
  :::
