---
title: 'reason()'
description: 'Gives the human message.'
---

# ExceptionLike.reason

<!-- signature: ExceptionLike['reason'] -->

```ts
reason(): string
```

Gives the human message. Synchronous.

**Parameters.** None.

**Returns.** `string`. The human message. Never parse it. Branch on `code()`.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { ExceptionLike } from '@fedimint/react-native'

declare const inner: ExceptionLike

const reason = inner.reason() // [!code focus]
console.log(reason)
```

**Used in.** [Errors, step 01](./errors.md#step-01), [Quickstart, step 02](./quickstart.md#step-02), [Installation, step 06](./installation.md#step-06)
