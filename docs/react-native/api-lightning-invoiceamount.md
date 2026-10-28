---
title: 'invoiceAmount()'
description: 'Gives the amount of the invoice.'
---

# LnQuoteLike.invoiceAmount

<!-- signature: LnQuoteLike['invoiceAmount'] -->

```ts
invoiceAmount(): Amount
```

Gives the amount of the invoice. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. What reaches the payee.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { LnQuoteLike } from '@fedimint/react-native'

declare const quote: LnQuoteLike

const msat = quote.invoiceAmount() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 04](./send.md#step-04)
