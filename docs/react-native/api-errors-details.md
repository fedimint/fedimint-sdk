---
title: 'details()'
description: 'Gives the structured detail of the failure, where there is any.'
---

# ExceptionLike.details

<!-- signature: ExceptionLike['details'] -->

```ts
details(): RawErrorDetails | undefined
```

Gives the structured detail of the failure, where there is any. Synchronous.

**Parameters.** None.

**Returns.** `RawErrorDetails | undefined`. The structured detail, or `undefined`.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { ExceptionLike } from '@fedimint/react-native'

declare const inner: ExceptionLike

const details = inner.details() // [!code focus]
console.log(details?.kind)
```

**Used in.** [Errors, step 01](./errors.md#step-01)
