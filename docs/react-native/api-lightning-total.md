---
title: 'total()'
description: 'Gives the total cost of the payment.'
---

# LnQuoteLike.total

<!-- signature: LnQuoteLike['total'] -->

```ts
total(): Amount
```

Gives the total cost of the payment. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. `invoiceAmount` plus `fee`. This is the exact debit.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { LnQuoteLike } from '@fedimint/react-native'

declare const quote: LnQuoteLike

const msat = quote.total() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 04](./send.md#step-04)
