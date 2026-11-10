---
title: 'details()'
description: 'Reads the details of an ecash send.'
---

# EcashSendOperationLike.details

<!-- signature: EcashSendOperationLike['details'] -->

```ts
details(): Promise<EcashSendDetails>
```

Reads the details of an ecash send.

**Parameters.** None.

**Returns.** `EcashSendDetails`. The details of the send.

**Throws.** [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed), [`Internal`](./errors.md#code-internal).

::: warning
The string is bearer money. Whoever has it can redeem it. Show it only to the recipient, and never log it, store it in plain text or send it to analytics.
:::

**Example**
{#example}

```ts [example.ts]
import type { EcashSendOperationLike } from '@fedimint/react-native'

declare const operation: EcashSendOperationLike
declare function showToRecipient(text: string): void

const details = await operation.details() // [!code focus]
showToRecipient(details.notes.display())
```

**Used in.** [History, step 04](./history.md#step-04)
