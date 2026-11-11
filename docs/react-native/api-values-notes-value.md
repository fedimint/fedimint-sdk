---
title: 'value()'
description: 'Reads the value inside the notes.'
---

# NotesLike.value

<!-- signature: NotesLike['value'] -->

```ts
value(): Amount
```

Reads the value inside the notes. It makes no network call. Synchronous.

**Parameters.** None.

**Returns.** `Amount`. The value in the notes. This does not prove that the notes are still unspent.

**Throws.** Not verified.

**Example**
{#example}

```ts [example.ts]
import type { NotesLike } from '@fedimint/react-native'

declare const notes: NotesLike

const msat = notes.value() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Receive, step 01](./receive.md#step-01), [Send, step 02](./send.md#step-02), [History, step 04](./history.md#step-04)
