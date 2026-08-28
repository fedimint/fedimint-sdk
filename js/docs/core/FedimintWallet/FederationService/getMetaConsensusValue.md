# Get Meta Consensus Value

### `federation.getMetaConsensusValue(key?: number)`

Reads federation metadata from the Meta module on an open client. The standard
federation metadata lives under `DEFAULT_META_KEY`.

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

type FederationMeta = {
  welcome_message?: string
  recurringd_api?: string
  lnaddress_api?: string
  'fedi:federation_icon_url'?: string
}

const director = new WalletDirector(new WasmWorkerTransport())
const wallet = await director.createWallet()

await wallet.open()

const metadata = await wallet.federation.getMetaConsensusValue<FederationMeta>()

const welcomeMessage = metadata?.value.welcome_message ?? null
const iconUrl = metadata?.value['fedi:federation_icon_url'] ?? null
```
