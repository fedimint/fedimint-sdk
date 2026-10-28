---
title: 'total()'
description: 'Gives the total cost of the quote.'
---

# EcashQuoteLike.total

<!-- signature: EcashQuoteLike['total'] -->

```ts
total(): Amount
```

Gives the total cost of the quote. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. `notesValue` plus `fee`. The balance moves by this amount.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { EcashQuoteLike } from '@fedimint/react-native'

declare const quote: EcashQuoteLike

const msat = quote.total() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 01](./send.md#step-01)
