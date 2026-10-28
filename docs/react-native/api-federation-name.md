---
title: 'name()'
description: 'Gives the name from the configuration of the federation.'
---

# FederationLike.name

<!-- signature: FederationLike['name'] -->

```ts
name(): string | undefined
```

Gives the name from the configuration of the `federation`. Synchronous.

**Parameters.** None.

**Returns.** `string | undefined`. The name from the configuration. It is not unique and it is not verified.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const name = fed.name() // [!code focus]
console.log(name ?? 'unnamed federation')
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02)
