# open

### `isOpen()`

Check if the wallet is open.

```ts twoslash
// @esModuleInterop
import { FedimintWallet } from '@fedimint/core-web'

const wallet = new FedimintWallet()

const isOpen = wallet.isOpen() // [!code focus]

if (!isOpen) {
  await wallet.joinFederation('fed123...')
} else {
  const balance = await wallet.balance.getBalance()
}
```
