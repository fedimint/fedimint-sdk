---
title: 'useOperation()'
description: 'A React hook that follows the state of an operation.'
---

# useOperation

<!-- signature: typeof useOperation -->

```ts
useOperation<S>(operation: ObservableOperation<S> | undefined, onError?: (err: unknown) => void): S | undefined
```

A React hook that follows the state of an operation.

**Parameters**

| Name        | Type                                  | Description                              |
| ----------- | ------------------------------------- | ---------------------------------------- |
| `operation` | `ObservableOperation<S> \| undefined` | Any generated operation handle.          |
| `onError`   | `(err: unknown) => void`              | Optional. Receives a failure to observe. |

**Returns.** `S | undefined`. The latest state of the operation. The final state stays after the stream ends.

**Throws.** The hook does not throw. A failure goes to onError and is never the outcome of the operation. The updates stream can report Storage, FederationClosed or Internal.

**Example**
{#example}

```tsx [example.tsx]
import {
  useOperation,
  type LnReceiveOperationLike,
} from '@fedimint/react-native'

export function useInvoiceState(op: LnReceiveOperationLike | undefined) {
  return useOperation(op, (err) => console.log('operation watch', err)) // [!code focus]
}
```

**Used in.** [Receive, step 04](./receive.md#step-04), [Session, step 03](./session.md#step-03)
