---
title: 'federation()'
description: 'Looks up an open federation by its id.'
---

# SdkLike.federation

<!-- signature: SdkLike['federation'] -->

```ts
federation(id: FederationId): FederationLike | undefined
```

Looks up an open `federation` by its id. Synchronous.

**Parameters**

| Name | Type           | Description                  |
| ---- | -------------- | ---------------------------- |
| `id` | `FederationId` | The `federation` to look up. |

**Returns.** `FederationLike | undefined`. The open `federation` handle, or `undefined` when none is open.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput). InvalidInput means the id is a malformed string.

**Example**
{#example}

```ts [example.ts]
import type { FederationId, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const id: FederationId

const fed = sdk.federation(id) // [!code focus]
console.log(fed?.name())
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02), [Join, step 03](./join.md#step-03), [Errors, step 01](./errors.md#step-01)
