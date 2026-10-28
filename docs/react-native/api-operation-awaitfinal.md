---
title: 'awaitFinal()'
description: 'Waits for the final state of the operation.'
---

# EcashSendOperationLike.awaitFinal

<!-- signature: EcashSendOperationLike['awaitFinal'] -->

```ts
awaitFinal(): Promise<EcashSendState>
```

Waits for the final state of the operation.

**Parameters.** None.

**Returns.** `EcashSendState`. The final state. A failed payment resolves with a failure state.

**Throws.** [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed), [`Internal`](./errors.md#code-internal). FederationClosed means the federation closed while the operation was pending.

**Example**
{#example}

```ts [example.ts]
import {
  EcashSendState,
  type EcashSendOperationLike,
} from '@fedimint/react-native'

declare const operation: EcashSendOperationLike

const final = await operation.awaitFinal() // [!code focus]
console.log(final === EcashSendState.Redeemed ? 'redeemed' : 'canceled')
```

**Used in.** [Send, step 03](./send.md#step-03), [Send, step 05](./send.md#step-05), [Receive, step 02](./receive.md#step-02), [Restore, step 03](./restore.md#step-03)
