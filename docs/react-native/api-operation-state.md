---
title: 'state()'
description: 'Reads the current state of the operation.'
---

# EcashSendOperationLike.state

<!-- signature: EcashSendOperationLike['state'] -->

```ts
state(): Promise<EcashSendState>
```

Reads the current state of the operation.

**Parameters.** None.

**Returns.** `EcashSendState`. The current state. A failed operation is a failure state, not an error.

**Throws.** [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed), [`Internal`](./errors.md#code-internal). This method never returns UnsupportedOperation.

**Example**
{#example}

```ts [example.ts]
import {
  EcashSendState,
  type EcashSendOperationLike,
} from '@fedimint/react-native'

declare const operation: EcashSendOperationLike

const state = await operation.state() // [!code focus]
console.log(EcashSendState[state])
```

**Used in.** [History, step 04](./history.md#step-04)
