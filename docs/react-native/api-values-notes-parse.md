---
title: 'parse()'
description: 'Checks an ecash string and gives you checked notes.'
---

# Notes.parse

<!-- signature: typeof Notes.parse -->

```ts
Notes.parse(notes: string): NotesLike
```

Checks an ecash string and gives you checked notes. Synchronous.

**Parameters**

| Name    | Type     | Description                     |
| ------- | -------- | ------------------------------- |
| `notes` | `string` | The ecash string from a sender. |

**Returns.** `NotesLike`. Checked notes.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput). InvalidInput means a malformed value.

**Example**
{#example}

```ts [example.ts]
import { Notes } from '@fedimint/react-native'

declare const pasted: string

const notes = Notes.parse(pasted.trim()) // [!code focus]
console.log(notes.value())
```

**Used in.** [Receive, step 01](./receive.md#step-01), [Receive, step 02](./receive.md#step-02)
