---
title: 'ecash()'
description: 'Gives the ecash facade.'
---

# FederationLike.ecash

<!-- signature: FederationLike['ecash'] -->

```ts
ecash(): EcashLike | undefined
```

Gives the ecash facade. Synchronous.

**Parameters.** None.

**Returns.** `EcashLike | undefined`. The facade, or `undefined` when the `federation` has no ecash module.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const ecash = fed.ecash() // [!code focus]
if (!ecash) throw new Error('This federation does not take ecash.')
```

**Used in.** [Send, step 01](./send.md#step-01), [Send, step 02](./send.md#step-02), [Receive, step 01](./receive.md#step-01), [Receive, step 02](./receive.md#step-02)
