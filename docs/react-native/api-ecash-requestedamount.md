---
title: 'requestedAmount()'
description: 'Gives the amount that was asked for.'
---

# EcashQuoteLike.requestedAmount

<!-- signature: EcashQuoteLike['requestedAmount'] -->

```ts
requestedAmount(): Amount
```

Gives the amount that was asked for. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. The amount that was asked for. It is a floor. It is not the amount the balance moves by.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { EcashQuoteLike } from '@fedimint/react-native'

declare const quote: EcashQuoteLike

const msat = quote.requestedAmount() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 01](./send.md#step-01)
