---
title: 'join()'
description: 'Joins a federation.'
---

# SdkLike.join

<!-- signature: SdkLike['join'] -->

```ts
join(invite: InviteCodeLike): Promise<FederationLike>
```

Joins a `federation`.

**Parameters**

| Name     | Type             | Description                                      |
| -------- | ---------------- | ------------------------------------------------ |
| `invite` | `InviteCodeLike` | The invite `code` of the `federation` to `join`. |

**Returns.** `FederationLike`. A handle to the joined `federation`. The `federation` is saved and running.

**Throws.** [`AlreadyJoined`](./errors.md#code-alreadyjoined), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`UnsupportedFederation`](./errors.md#code-unsupportedfederation), [`FederationClosed`](./errors.md#code-federationclosed), [`Storage`](./errors.md#code-storage). It also returns every error of preview.

**Example**
{#example}

```ts [example.ts]
import type { InviteCodeLike, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const invite: InviteCodeLike

const fed = await sdk.join(invite) // [!code focus]
console.log(fed.id())
```

**Used in.** [Join, step 03](./join.md#step-03), [Quickstart, step 02](./quickstart.md#step-02)
