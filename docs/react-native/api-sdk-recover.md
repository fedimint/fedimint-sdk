---
title: 'recover()'
description: 'Joins a federation again and restores its funds from the seed.'
---

# SdkLike.recover

<!-- signature: SdkLike['recover'] -->

```ts
recover(invite: InviteCodeLike): Promise<RecoveryHandle>
```

Joins a `federation` again and restores its funds from the seed.

**Parameters**

| Name     | Type             | Description                                                     |
| -------- | ---------------- | --------------------------------------------------------------- |
| `invite` | `InviteCodeLike` | The invite `code` of the `federation` to restore from the seed. |

**Returns.** `RecoveryHandle`. An object with the `federation` handle and an operation that reports the progress of the recovery.

**Throws.** [`AlreadyJoined`](./errors.md#code-alreadyjoined), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`UnsupportedFederation`](./errors.md#code-unsupportedfederation), [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed). Each code has the caveat that the wallet may already have joined.

**Example**
{#example}

```ts [example.ts]
import type { InviteCodeLike, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const invite: InviteCodeLike

const { progress } = await sdk.recover(invite) // [!code focus]
console.log((await progress.awaitFinal()).tag)
```

**Used in.** [Restore, step 03](./restore.md#step-03)
