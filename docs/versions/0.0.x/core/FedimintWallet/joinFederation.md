# joinFederation

### `joinFederation(federationId: string)`

Attempts to join a federation.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const didJoin = await wallet.joinFederation('fed123...') // [!code focus]

if (didJoin) {
  const balance = await wallet.balance.getBalance()
}
```

To support multiple wallets within a single application, you can pass in a custom client name.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const didJoin = await wallet.joinFederation('fed456...', 'my-client-name') // [!code focus]
```
