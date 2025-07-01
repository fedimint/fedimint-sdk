# Get Operation

### `federation.getOperation(operationId:string)`

Get the transaction data using the operation id.

```ts twoslash
// @esModuleInterop
import { FedimintWallet } from '@fedimint/core-web'

const wallet = new FedimintWallet()
wallet.open()

const operationId =
  '3ff56b29cf014b9ff6c8b6b4aa78e02d3c429de7112bfaf42a876f6a797ddf8b'
const config = await wallet.federation.getOperation(operationId)
```
