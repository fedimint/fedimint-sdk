---
title: 'updates()'
description: 'Opens a cursor on the states of the operation.'
---

# EcashSendOperationLike.updates

<!-- signature: EcashSendOperationLike['updates'] -->

```ts
updates(): EcashSendOperationUpdatesLike
```

Opens a cursor on the states of the operation. Synchronous.

**Parameters.** None.

**Returns.** `EcashSendOperationUpdatesLike`. A cursor. Its `next()` gives Promise&lt;State | `undefined`&gt;: the current state first, then each change, and `undefined` at the clean end.

**Throws.** Not verified. The next() call gives Storage, FederationClosed or Internal.

**Example**
{#example}

```ts [example.ts]
import type { EcashSendOperationLike } from '@fedimint/react-native'

declare const operation: EcashSendOperationLike

const updates = operation.updates() // [!code focus]
const state = await updates.next()
console.log(state)
```

**Used in.** [Receive, step 02](./receive.md#step-02), [Receive, step 04](./receive.md#step-04), [Restore, step 03](./restore.md#step-03)
