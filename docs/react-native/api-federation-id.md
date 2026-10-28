---
title: 'id()'
description: 'Gives the id of this federation.'
---

# FederationLike.id

<!-- signature: FederationLike['id'] -->

```ts
id(): FederationId
```

Gives the id of this `federation`. Synchronous.

**Parameters.** None.

**Returns.** `FederationId`. The id of the `federation`, as a string.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const id = fed.id() // [!code focus]
console.log(id)
```

**Used in.** [Amounts, step 02](./amounts.md#step-02), [Restore, step 04](./restore.md#step-04)
