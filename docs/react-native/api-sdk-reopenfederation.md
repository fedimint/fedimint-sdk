---
title: 'reopenFederation()'
description: 'Starts a stored federation again, without an invite code.'
---

# SdkLike.reopenFederation

<!-- signature: SdkLike['reopenFederation'] -->

```ts
reopenFederation(id: FederationId): Promise<FederationLike>
```

Starts a stored `federation` again, without an invite `code`.

**Parameters**

| Name | Type           | Description                             |
| ---- | -------------- | --------------------------------------- |
| `id` | `FederationId` | The stored `federation` to start again. |

**Returns.** `FederationLike`. The live handle. Old handles stay closed.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput), [`UnsupportedFederation`](./errors.md#code-unsupportedfederation), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed). InvalidInput means no such federation exists, or it is in the Forgetting state.

**Example**
{#example}

```ts [example.ts]
import type { FederationId, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const id: FederationId

const fed = await sdk.reopenFederation(id) // [!code focus]
console.log(fed.name())
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02), [Join, step 03](./join.md#step-03), [Errors, step 01](./errors.md#step-01)
