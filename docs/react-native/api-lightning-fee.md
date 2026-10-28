---
title: 'fee()'
description: 'Gives the fee of the payment.'
---

# LnQuoteLike.fee

<!-- signature: LnQuoteLike['fee'] -->

```ts
fee(): Amount
```

Gives the `fee` of the payment. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. The whole extra cost.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { LnQuoteLike } from '@fedimint/react-native'

declare const quote: LnQuoteLike

const msat = quote.fee() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 04](./send.md#step-04)
