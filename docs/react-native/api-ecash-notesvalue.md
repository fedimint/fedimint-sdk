---
title: 'notesValue()'
description: 'Gives the value of the notes in the quote.'
---

# EcashQuoteLike.notesValue

<!-- signature: EcashQuoteLike['notesValue'] -->

```ts
notesValue(): Amount
```

Gives the value of the notes in the quote. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. The value the notes carry. It is at or above the requested amount.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { EcashQuoteLike } from '@fedimint/react-native'

declare const quote: EcashQuoteLike

const msat = quote.notesValue() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Send, step 01](./send.md#step-01)
