# generateAddress

### `wallet.generateAddress()`

Create an onchain address. Returns a `deposit_address` (string) and an `operation_id` (string).

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const { deposit_address, operation_id } = await wallet.wallet.generateAddress() // [!code focus]
```
