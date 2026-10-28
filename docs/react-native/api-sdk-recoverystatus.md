---
title: 'recoveryStatus()'
description: 'Reads the recovery state of a federation.'
---

# SdkLike.recoveryStatus

<!-- signature: SdkLike['recoveryStatus'] -->

```ts
recoveryStatus(id: FederationId): Promise<RecoveryState | undefined>
```

Reads the recovery state of a `federation`.

**Parameters**

| Name | Type           | Description                    |
| ---- | -------------- | ------------------------------ |
| `id` | `FederationId` | The `federation` to ask about. |

**Returns.** `RecoveryState | undefined`. The recovery state. It is `undefined` when the `federation` has no recovery.

**Throws.** [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed). FederationClosed means the id names no open federation, or the SDK is shut down.

**Example**
{#example}

```ts [example.ts]
import type { FederationId, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const id: FederationId

const state = await sdk.recoveryStatus(id) // [!code focus]
console.log(state?.tag)
```

**Used in.** [Restore, step 04](./restore.md#step-04)
