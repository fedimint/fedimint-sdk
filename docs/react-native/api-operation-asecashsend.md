---
title: 'asEcashSend()'
description: 'Turns an untyped operation into an ecash send handle.'
---

# AnyOperationLike.asEcashSend

<!-- signature: AnyOperationLike['asEcashSend'] -->

```ts
asEcashSend(): EcashSendOperationLike | undefined
```

Turns an untyped operation into an ecash send handle. Synchronous.

**Parameters.** None.

**Returns.** `EcashSendOperationLike | undefined`. A typed handle when the operation is an ecash send. Otherwise `undefined`.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { AnyOperationLike } from '@fedimint/react-native'

declare const any: AnyOperationLike

const send = any.asEcashSend() // [!code focus]
if (send) console.log(await send.state())
```

**Used in.** [History, step 04](./history.md#step-04)
