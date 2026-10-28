---
title: 'network()'
description: 'Gives the Bitcoin network of the federation.'
---

# FederationLike.network

<!-- signature: FederationLike['network'] -->

```ts
network(): Network
```

Gives the Bitcoin network of the `federation`. Synchronous.

**Parameters.** None.

**Returns.** `Network`. The Bitcoin network the `federation` runs on.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import { Network, type FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const network = fed.network() // [!code focus]
console.log(network === Network.Bitcoin ? 'real money' : 'test money')
```

**Used in.** [Receive, step 03](./receive.md#step-03)
