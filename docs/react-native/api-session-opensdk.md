---
title: 'openSdk()'
description: 'Opens the SDK on a data directory and gives you a session.'
---

# openSdk

<!-- signature: typeof openSdk -->

```ts
openSdk(options: OpenOptions): Promise<SdkSession>
```

Opens the SDK on a data directory and gives you a session.

**Parameters**

| Name               | Type       | Description                                                                                                         |
| ------------------ | ---------- | ------------------------------------------------------------------------------------------------------------------- |
| `options.dataDir`  | `string`   | The directory where the SDK keeps its data.                                                                         |
| `options.mnemonic` | `string[]` | Optional. Seed `words` to establish. Omit it to use the stored seed, or to make a new seed over an empty directory. |

**Returns.** `SdkSession`. An open session. It holds `sdk`, `InviteCode.parse`, `Notes.parse`, `Mnemonic.fromWords`, `Mnemonic.generate` and `close()`.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput), [`StorageInUse`](./errors.md#code-storageinuse), [`SeedMismatch`](./errors.md#code-seedmismatch), [`Entropy`](./errors.md#code-entropy), [`Storage`](./errors.md#code-storage), [`StorageOrphaned`](./errors.md#code-storageorphaned). InvalidInput means bad seed words. This call does not return UnsupportedFederation or FederationUnreachable.

**Example**
{#example}

```ts [example.ts]
import { openSdk } from '@fedimint/react-native'

declare const dataDir: string

const session = await openSdk({ dataDir }) // [!code focus]
console.log(session.sdk.federations().length)
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02), [Installation, step 06](./installation.md#step-06), [Session, step 01](./session.md#step-01), [Seed words, step 01](./seed-words.md#step-01), [Restore, step 02](./restore.md#step-02)
