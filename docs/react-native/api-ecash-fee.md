---
title: 'fee()'
description: 'Gives the fee of the quote.'
---

# EcashQuoteLike.fee

<!-- signature: EcashQuoteLike['fee'] -->

```ts
fee(): Amount
```

Gives the `fee` of the quote. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. The extra cost on top of `notesValue`. It is always zero today.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { EcashQuoteLike } from '@fedimint/react-native'

declare const quote: EcashQuoteLike

const msat = quote.fee() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 01](./send.md#step-01)
