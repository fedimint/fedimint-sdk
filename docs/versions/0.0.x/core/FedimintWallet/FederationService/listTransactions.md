# List Transactions

### `federation.listTransactions(limit?:number,lastseen:lastSeenRequest)`

Returns a paginated list of transactions from the federation. In case limit and lastseen not given, returns all the available transactions.

```ts twoslash
/**
 * Represents wallet transaction record.
 * Includes timestamp, type, amount, and related metadata.
 */
export type Transactions = {
  timeStamp: string
  paymentType: string
  type: string
  amount: string
  invoice: string
  operationId: string
  outcome: string
  gateway: string
}
```

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const limit = 10
const lastseen = {
  creation_time: { secs_since_epoch: 2323233, nanos_since_epoch: 93429234 },
  operation_id:
    '3ff56b29cf014b9ff6c8b6b4aa78e02d3c429de7112bfaf42a876f6a797ddf8b',
}
const transactions = await wallet.federation.listTransactions(limit, lastseen)
console.log('transactions are: ', transactions)
```
