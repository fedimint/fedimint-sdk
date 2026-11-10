---
title: 'display()'
description: 'Gives the string to hand over.'
---

# NotesLike.display

<!-- signature: NotesLike['display'] -->

```ts
display(): string
```

Gives the string to hand over. Synchronous.

**Parameters.** None.

**Returns.** `string`. The ecash string for the notes.

**Throws.** Not verified.

::: warning
The string is bearer money. Whoever has it can redeem it. Show it only to the recipient, and never log it, store it in plain text or send it to analytics.
:::

**Example**
{#example}

```ts [example.ts]
import type { NotesLike } from '@fedimint/react-native'

declare const notes: NotesLike
declare function showToRecipient(text: string): void

const text = notes.display() // [!code focus]
showToRecipient(text)
```

**Used in.** [Receive, step 01](./receive.md#step-01), [Send, step 02](./send.md#step-02), [History, step 04](./history.md#step-04)
