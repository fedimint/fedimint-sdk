---
title: 'code()'
description: 'Gives the stable error code.'
---

# ExceptionLike.code

<!-- signature: ExceptionLike['code'] -->

```ts
code(): ErrorCode
```

Gives the stable error `code`. Synchronous.

**Parameters.** None.

**Returns.** `ErrorCode`. The stable `code`. Branch on it.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import { ErrorCode, type ExceptionLike } from '@fedimint/react-native'

declare const inner: ExceptionLike

const code = inner.code() // [!code focus]
console.log(ErrorCode[code])
```

**Used in.** [Errors, step 01](./errors.md#step-01)
