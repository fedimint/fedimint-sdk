# Subscribe Balance

### `balance.subscribeBalance()` <Badge type="info" text="Streaming" />

Subscribe to balance updates as they occur.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const unsubscribe = wallet.balance.subscribeBalance((mSats) => { // [!code focus]
  console.log('Balance updated:', mSats) // [!code focus]
  // 1000 mSats = 1 satoshi // [!code focus]
}) // [!code focus]

// ...Cleanup Later
unsubscribe()
```
