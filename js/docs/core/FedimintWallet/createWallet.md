# Creating a FedimintWallet

Create a `WalletDirector` for the current platform, then ask it to create the
wallet. Do not call the `FedimintWallet` constructor directly.

```ts twoslash
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet() // [!code focus]
```

`createWallet()` waits for the director's transport initialization and returns a
new, unopened `FedimintWallet`. Then either open existing client state or join a
federation:

::: warning Lazy initialization with a custom database path
When a lazy director uses a custom `dbPath`, initialize it explicitly before
calling `createWallet()`. Deferred initialization does not automatically forward
the path configured on the director.

```ts twoslash
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const dbPath = 'custom.db'
const director = new WalletDirector(new WasmWorkerTransport(), dbPath, true)
await director.initialize(dbPath)
const wallet = await director.createWallet()
```

:::

```ts twoslash
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

// Alternatively, on an unopened wallet:
// const didJoin = await wallet.joinFederation('fed11...')
```

## Using the wallet type

Import `FedimintWallet` as a type when an explicit annotation is useful:

```ts twoslash
import { type FedimintWallet, WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
const wallet: FedimintWallet = await director.createWallet()
```
