---
title: 'activity()'
description: 'Reads one page of the local history.'
---

# FederationLike.activity

<!-- signature: FederationLike['activity'] -->

```ts
activity(cursor: Cursor | undefined, limit: number): Promise<ActivityPage>
```

Reads one page of the local history.

**Parameters**

| Name     | Type                  | Description                                                                                                    |
| -------- | --------------------- | -------------------------------------------------------------------------------------------------------------- |
| `cursor` | `Cursor \| undefined` | Use `undefined` for the first page. After that, use the `next` value of the previous page. Cursor is a string. |
| `limit`  | `number`              | The maximum number of items.                                                                                   |

**Returns.** `ActivityPage`. One page of items, newest first, with an optional `next` cursor. A page with fewer items than `limit` is normal. An empty page with no `next` cursor is the end.

**Throws.** [`Storage`](./errors.md#code-storage), [`InvalidInput`](./errors.md#code-invalidinput), [`FederationClosed`](./errors.md#code-federationclosed). InvalidInput means a cursor from another source, or a limit of 0.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const page = await fed.activity(undefined, 20) // [!code focus]
console.log(page.items.length, page.next)
```

**Used in.** [History, step 01](./history.md#step-01), [History, step 02](./history.md#step-02)
