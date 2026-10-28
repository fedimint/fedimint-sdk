---
title: 'operation()'
description: 'Looks up one operation by its id.'
---

# FederationLike.operation

<!-- signature: FederationLike['operation'] -->

```ts
operation(id: OperationId): Promise<AnyOperationLike | undefined>
```

Looks up one operation by its id.

**Parameters**

| Name | Type          | Description                                        |
| ---- | ------------- | -------------------------------------------------- |
| `id` | `OperationId` | The operation to look up. OperationId is a string. |

**Returns.** `AnyOperationLike | undefined`. The operation, or `undefined` when the log has no operation with that id.

**Throws.** [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed).

**Example**
{#example}

```ts [example.ts]
import type { FederationLike, OperationId } from '@fedimint/react-native'

declare const fed: FederationLike
declare const operationId: OperationId

const operation = await fed.operation(operationId) // [!code focus]
console.log(operation?.kind())
```

**Used in.** [History, step 04](./history.md#step-04)
