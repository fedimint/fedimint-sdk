---
title: 'parse()'
description: 'Checks an invite code string and gives you a checked invite code.'
---

# InviteCode.parse

<!-- signature: typeof InviteCode.parse -->

```ts
InviteCode.parse(code: string): InviteCodeLike
```

Checks an invite `code` string and gives you a checked invite `code`. Synchronous.

**Parameters**

| Name   | Type     | Description           |
| ------ | -------- | --------------------- |
| `code` | `string` | The `fed1...` string. |

**Returns.** `InviteCodeLike`. A checked invite `code`.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput). InvalidInput means a malformed value. The error does not echo the string back.

**Example**
{#example}

```ts [example.ts]
import { InviteCode } from '@fedimint/react-native'

declare const pasted: string

const invite = InviteCode.parse(pasted.trim()) // [!code focus]
console.log(invite.federationId())
```

**Used in.** [Join, step 01](./join.md#step-01), [Quickstart, step 02](./quickstart.md#step-02), [Errors, step 01](./errors.md#step-01), [Restore, step 03](./restore.md#step-03), [Receive (prose)](./receive.md)
