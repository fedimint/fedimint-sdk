---
title: 'hasInner()'
description: 'Tells whether a caught value carries an SDK Exception.'
---

# Exception.hasInner

```ts no-check
Exception.hasInner(obj_: any): obj_ is UniffiThrownObject<Exception>
```

This block has data-verify="none" because UniffiThrownObject comes from `@ubjs/core`. The package does not export it.

Tells whether a caught value carries an SDK `Exception`. Synchronous.

**Parameters**

| Name   | Type  | Description       |
| ------ | ----- | ----------------- |
| `obj_` | `any` | The caught value. |

**Returns.** `boolean (type guard)`. True when the caught value carries an SDK `Exception`.

**Throws.** Not verified.

**Example**
{#example}

```ts [example.ts]
import { Exception } from '@fedimint/react-native'

declare const err: unknown

if (Exception.hasInner(err)) {
  // [!code focus]
  console.log(Exception.getInner(err).reason())
}
```

**Used in.** [Errors, step 01](./errors.md#step-01), [Quickstart, step 02](./quickstart.md#step-02), [Join, step 01](./join.md#step-01), [Send, step 02](./send.md#step-02), [Send, step 04](./send.md#step-04), [Restore, step 02](./restore.md#step-02), [Session, step 03](./session.md#step-03), [Installation, step 06](./installation.md#step-06)
