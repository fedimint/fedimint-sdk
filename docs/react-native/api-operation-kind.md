---
title: 'kind()'
description: 'Tells the kind of an operation.'
---

# AnyOperationLike.kind

<!-- signature: AnyOperationLike['kind'] -->

```ts
kind(): OperationKind
```

Tells the kind of an operation. Synchronous.

**Parameters.** None.

**Returns.** `OperationKind`. The kind of the operation. An unknown record gives `OperationKind.Unknown`.

**Throws.** None. The method never fails.

**Example**
{#example}

```ts [example.ts]
import { OperationKind, type AnyOperationLike } from '@fedimint/react-native'

declare const any: AnyOperationLike

const kind = any.kind() // [!code focus]
console.log(OperationKind[kind])
```

**Used in.** [History, step 04](./history.md#step-04)
