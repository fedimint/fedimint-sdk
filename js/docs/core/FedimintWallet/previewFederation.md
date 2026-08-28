# previewFederation

### `previewFederation(inviteCode: string): Promise<PreviewFederation>`

Attempts to retrieve federation details before joining a Federation. This allows you to inspect the federation's configuration, endpoints, modules, and metadata without committing to join.

#### Parameters

- `inviteCode` - The federation invite code to preview

#### Returns

`PreviewFederation` - An object containing:

```ts
{
  config: JsonClientConfig // Detailed federation configuration
  federation_id: string // Federation identifier
}
```

The `config` object includes:

- `global`: Global configuration including:
  - `api_endpoints`: Record of peer URLs and names
  - `broadcast_public_keys`: Public keys for each peer
  - `consensus_version`: The consensus version (major/minor)
  - `meta`: Metadata including federation name
- `modules`: Record of module configurations (e.g., "ln", "mint", "wallet")

#### Example

```ts twoslash
// @esModuleInterop
import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())

const preview = await director.previewFederation('fed123...') // [!code focus]

// Access federation details
console.log('Federation ID:', preview.federation_id)
console.log('Federation Name:', preview.config.global.meta.federation_name)
console.log('Consensus Version:', preview.config.global.consensus_version)

// Access API endpoints
Object.entries(preview.config.global.api_endpoints).forEach(
  ([peerId, peer]) => {
    console.log(`Peer ${peerId}: ${peer.name} at ${peer.url}`)
  },
)

// Check available modules
console.log('Available modules:', Object.keys(preview.config.modules))
```
