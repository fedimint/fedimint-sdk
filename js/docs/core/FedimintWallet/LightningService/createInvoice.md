# Receive Lightning

### `lightning.createInvoice()`

Create a Lightning Invoice for a given amount. Returns a `CreateBolt11Response` object containing details about the created invoice.

You can use `subscribeLnReceive` to track the invoice status.

`waitForReceive` returns a `Promise` that resolves when the invoice succeeds or `timeoutMs` is reached.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'
import type { LnReceiveState } from '@fedimint/core'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

wallet.open()

const { operation_id, invoice } = await wallet.lightning.createInvoice( // [!code focus]
  10_000, // msats // [!code focus]
  'This is an invoice description', // [!code focus]
) // [!code focus]

console.log(operation_id) // operation id for the invoice
console.log(invoice) // bolt11 invoice

const unsubscribe = wallet.lightning.subscribeLnReceive( // [!code focus]
  operation_id, // [!code focus]
  (state: LnReceiveState) => console.log(state), // [!code focus]
  (error: string) => console.error(error), // [!code focus]
) // [!code focus]

// ...Cleanup Later
unsubscribe()

try {
  const timeoutMs = 10000
  await wallet.lightning.waitForReceive(operation_id, timeoutMs) // [!code focus]
  console.log('Payment Received!')
} catch (error) {
  console.error(error) // Timeout waiting for payment
}
```
