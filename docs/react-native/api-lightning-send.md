---
title: 'send()'
description: 'Pays the invoice of a quote.'
---

# LightningLike.send

<!-- signature: LightningLike['send'] -->

```ts
send(quote: LnQuoteLike): Promise<LnSendOperationLike>
```

Pays the invoice of a quote.

**Parameters**

| Name    | Type          | Description                                                                |
| ------- | ------------- | -------------------------------------------------------------------------- |
| `quote` | `LnQuoteLike` | The quote to pay. A quote that was already sent fails with `QuoteExpired`. |

**Returns.** `LnSendOperationLike`. The operation. It returns when the payment has started, not when it ends. The final state is `Success`, `Refunded` or `Failed`.

**Throws.** [`QuoteExpired`](./errors.md#code-quoteexpired), [`QuoteChanged`](./errors.md#code-quotechanged), [`NetworkMismatch`](./errors.md#code-networkmismatch), [`InsufficientBalance`](./errors.md#code-insufficientbalance), [`GatewayUnavailable`](./errors.md#code-gatewayunavailable), [`Recovering`](./errors.md#code-recovering), [`NotSupported`](./errors.md#code-notsupported), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`Storage`](./errors.md#code-storage), [`Internal`](./errors.md#code-internal), [`FederationClosed`](./errors.md#code-federationclosed).

**Example**
{#example}

```ts [example.ts]
import type { LightningLike, LnQuoteLike } from '@fedimint/react-native'

declare const ln: LightningLike
declare const quote: LnQuoteLike

const operation = await ln.send(quote) // [!code focus]
console.log((await operation.awaitFinal()).tag)
```

**Used in.** [Send, step 05](./send.md#step-05)
