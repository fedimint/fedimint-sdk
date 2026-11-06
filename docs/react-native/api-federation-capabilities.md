---
title: 'capabilities()'
description: 'Tells what the federation can do.'
---

# FederationLike.capabilities

<!-- signature: FederationLike['capabilities'] -->

```ts
capabilities(): Capabilities
```

Tells what the `federation` can do. Synchronous.

**Parameters.** None.

**Returns.** `Capabilities`. An object of booleans, such as ecash, for what the `federation` can do.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const caps = fed.capabilities() // [!code focus]
console.log(caps.ecash, caps.lightning, caps.onchain)
```

**Used in.** [Join, step 03](./join.md#step-03)
