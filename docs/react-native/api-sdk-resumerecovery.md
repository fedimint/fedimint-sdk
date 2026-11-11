---
title: 'resumeRecovery()'
description: 'Continues the recovery of a federation.'
---

# SdkLike.resumeRecovery

<!-- signature: SdkLike['resumeRecovery'] -->

```ts
resumeRecovery(id: FederationId): Promise<RecoveryHandle>
```

Continues the recovery of a `federation`.

**Parameters**

| Name | Type           | Description                                                          |
| ---- | -------------- | -------------------------------------------------------------------- |
| `id` | `FederationId` | The `federation` whose recovery continues. FederationId is a string. |

**Returns.** `RecoveryHandle`. A handle to the `federation` and its recovery operation.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput), [`FederationClosed`](./errors.md#code-federationclosed), [`Storage`](./errors.md#code-storage). InvalidInput means the wallet joined this federation with join, so no recovery record exists. FederationClosed means the id names no open federation, or the SDK is shut down. Not verified: more codes can exist past these three.

**Example**
{#example}

```ts [example.ts]
import type { FederationId, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const id: FederationId

const { progress } = await sdk.resumeRecovery(id) // [!code focus]
console.log((await progress.awaitFinal()).tag)
```

**Used in.** [Restore, step 04](./restore.md#step-04)
