# Redeem Ecash

### `federation.getFederationId()`

Access the `federationId` of the connected Federation.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const config = await wallet.federation.getFederationId() // [!code focus]
```
