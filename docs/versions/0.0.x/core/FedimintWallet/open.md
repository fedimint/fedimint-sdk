# open

### `open(clientName?: string)`

Attempts to open an existing wallet.

Default client name is `fm-wallet`.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const isOpen = await wallet.open() // [!code focus]

if (isOpen) {
  const balance = await wallet.balance.getBalance()
} else {
  await wallet.joinFederation('fed123...')
}
```

To support multiple wallets within a single application, you can pass in a custom client name.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const isOpen = await wallet.open('my-client-name') // [!code focus]
```
