---
title: 'lightning()'
description: 'Gives the Lightning facade.'
---

# FederationLike.lightning

<!-- signature: FederationLike['lightning'] -->

```ts
lightning(): LightningLike | undefined
```

Gives the Lightning facade. Synchronous.

**Parameters.** None.

**Returns.** `LightningLike | undefined`. The facade, or `undefined` when the `federation` has no Lightning module.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const ln = fed.lightning() // [!code focus]
if (!ln) throw new Error('This federation has no Lightning module.')
```

**Used in.** [Receive, step 03](./receive.md#step-03), [Send, step 04](./send.md#step-04), [Send, step 05](./send.md#step-05)
