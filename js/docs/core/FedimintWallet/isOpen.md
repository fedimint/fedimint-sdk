# isOpen

### `isOpen()`

Check if the wallet is open.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const isOpen = wallet.isOpen() // [!code focus]

if (!isOpen) {
  await wallet.joinFederation('fed123...')
} else {
  const balance = await wallet.balance.getBalance()
}
```
