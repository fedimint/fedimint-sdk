# setLogLevel

### `setLogLevel()`

Set the log level for the walletDirector.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const walletDirector = new WalletDirector(new WasmWorkerTransport())

walletDirector.setLogLevel('debug') // [!code focus]
```
