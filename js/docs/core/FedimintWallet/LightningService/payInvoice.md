# Send Lightning

### `lightning.payInvoiceSync(invoice: string)`

Attempts to pay an invoice. Returns a `Promise` that resolves when the payment succeeds or fails / times out.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const result = await wallet.lightning.payInvoiceSync( // [!code focus]
  'lnbc...', // bolt11 invoice // [!code focus]
) // [!code focus]

if (result.success) {
  console.log(result.data.preimage) // preimage of the settled payment
  console.log(result.data.feeMsats) // fee paid in msats
}
```

### `lightning.payInvoice(invoice: string)`

Attempts to pay a lightning invoice. Returns an `OutgoingLightningPayment` object containing details about the in-flight payment.

You can use `subscribeLnPay` and `subscribeInternalPay` to track the payment status. `waitForPay` returns a `Promise` that resolves when the payment succeeds or fails / times out.

`subscribeInternalPay` can be used to track the internal payments status

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'
import type { LnPayState } from '@fedimint/core'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const { fee, payment_type } = await wallet.lightning.payInvoice( // [!code focus]
  'lnbc...', // bolt11 invoice // [!code focus]
) // [!code focus]

// TODO: Simplify and explain this. 
// Users should not have to check if the operation_id exists.

let id = ''; 
if ('lightning' in payment_type) {
  id = payment_type.lightning // in flight lightning payment id
}

const unsubscribe = wallet.lightning.subscribeLnPay( // [!code focus]
  id, // [!code focus]
  (state: LnPayState) => console.log(state), // State of the payment // [!code focus]
  (error: string) => console.error(error), // [!code focus]
)

// ...Cleanup Later
unsubscribe()

const result = await wallet.lightning.waitForPay(id) // [!code focus]

if (result.success) {
  console.log(result.data.preimage) // preimage of the settled payment
}
```
