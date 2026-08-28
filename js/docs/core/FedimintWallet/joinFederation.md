# joinFederation

### `joinFederation(inviteCode: string, clientNameOrOptions?: string | JoinFederationOptions)`

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

To support multiple wallets within a single application, pass a custom client name.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const didJoin = await wallet.joinFederation('fed456...', 'my-client-name') // [!code focus]
```

You can also pass an options object. This is the preferred form when setting `forceRecover`.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const didJoin = await wallet.joinFederation('fed456...', {
  clientName: 'my-client-name',
}) // [!code focus]
```

To force recovery while joining, set `forceRecover`.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

const didJoin = await wallet.joinFederation('fed789...', {
  forceRecover: true,
}) // [!code focus]
```
