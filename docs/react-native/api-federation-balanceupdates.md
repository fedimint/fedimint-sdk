---
title: 'balanceUpdates()'
description: 'Opens a new cursor on the balance.'
---

# FederationLike.balanceUpdates

<!-- signature: FederationLike['balanceUpdates'] -->

```ts
balanceUpdates(): BalanceUpdatesLike
```

Opens a new cursor on the balance. Synchronous.

**Parameters.** None.

**Returns.** `BalanceUpdatesLike`. A new cursor. Call `next()` for each value. The first `next()` gives the current value.

**Throws.** This call cannot fail. The first next() gives FederationClosed on a closed federation. Later next() calls can also give Storage or Internal.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const updates = fed.balanceUpdates() // [!code focus]
const msat = await updates.next()
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Balance, step 02](./balance.md#step-02)
