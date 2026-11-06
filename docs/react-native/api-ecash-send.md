---
title: 'send()'
description: 'Runs a quote and makes the notes to hand over.'
---

# EcashLike.send

<!-- signature: EcashLike['send'] -->

```ts
send(quote: EcashQuoteLike): Promise<EcashSendHandle>
```

Runs a quote and makes the notes to hand over.

**Parameters**

| Name    | Type             | Description                                                                |
| ------- | ---------------- | -------------------------------------------------------------------------- |
| `quote` | `EcashQuoteLike` | The quote to run. A quote that was already sent fails with `QuoteExpired`. |

**Returns.** `EcashSendHandle`. An object with the notes to hand over and the operation to follow. The balance drops when this call returns.

**Throws.** [`QuoteExpired`](./errors.md#code-quoteexpired), [`QuoteChanged`](./errors.md#code-quotechanged), [`InsufficientBalance`](./errors.md#code-insufficientbalance), [`Recovering`](./errors.md#code-recovering), [`NotSupported`](./errors.md#code-notsupported), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed).

**Example**
{#example}

```ts [example.ts]
import type { EcashLike, EcashQuoteLike } from '@fedimint/react-native'

declare const ecash: EcashLike
declare const quote: EcashQuoteLike

const { notes, operation } = await ecash.send(quote) // [!code focus]
console.log(notes.display(), operation !== undefined)
```

**Used in.** [Send, step 02](./send.md#step-02)
