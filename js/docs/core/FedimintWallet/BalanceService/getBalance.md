# Get Balance

### `balance.getBalance()`

Get the current balance of the wallet in milli-satoshis (MSats).

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const mSats = await wallet.balance.getBalance() // [!code focus]

// 1000 mSats = 1 satoshi
```
