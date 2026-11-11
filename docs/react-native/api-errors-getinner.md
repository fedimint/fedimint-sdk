---
title: 'getInner()'
description: 'Gives the Exception inside a caught value.'
---

# Exception.getInner

```ts no-check
Exception.getInner(err: UniffiThrownObject<Exception>): Exception
```

This block has data-verify="none" because UniffiThrownObject comes from `@ubjs/core`. The package does not export it.

Gives the `Exception` inside a caught value. Synchronous.

**Parameters**

| Name  | Type                            | Description                     |
| ----- | ------------------------------- | ------------------------------- |
| `err` | `UniffiThrownObject<Exception>` | A value that passed `hasInner`. |

**Returns.** `Exception`. The SDK exception object.

**Throws.** Not verified.

**Example**
{#example}

```ts [example.ts]
import { Exception } from '@fedimint/react-native'

declare const err: unknown

if (Exception.hasInner(err)) {
  const inner = Exception.getInner(err) // [!code focus]
  console.log(inner.code())
}
```

**Used in.** [Errors, step 01](./errors.md#step-01), [Quickstart, step 02](./quickstart.md#step-02), [Join, step 01](./join.md#step-01), [Send, step 02](./send.md#step-02), [Send, step 04](./send.md#step-04), [Restore, step 02](./restore.md#step-02), [Session, step 03](./session.md#step-03), [Installation, step 06](./installation.md#step-06)
