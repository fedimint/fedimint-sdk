---
title: 'onchain()'
description: 'Gives the on-chain facade.'
---

# FederationLike.onchain

<!-- signature: FederationLike['onchain'] -->

```ts
onchain(): OnchainLike | undefined
```

Gives the on-chain facade. Synchronous.

**Parameters.** None.

**Returns.** `OnchainLike | undefined`. The facade, or `undefined` when the `federation` has no on-chain module.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const onchain = fed.onchain() // [!code focus]
console.log(onchain === undefined ? 'no on-chain module' : 'on-chain module')
```

**Used in.** [Join, step 03](./join.md#step-03)
