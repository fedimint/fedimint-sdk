---
title: 'quote()'
description: 'Plans the payment of an invoice.'
---

# LightningLike.quote

<!-- signature: LightningLike['quote'] -->

```ts
quote(invoice: Bolt11Invoice): Promise<LnQuoteLike>
```

Plans the payment of an invoice. Nothing is debited.

**Parameters**

| Name      | Type            | Description                                                            |
| --------- | --------------- | ---------------------------------------------------------------------- |
| `invoice` | `Bolt11Invoice` | The invoice to pay. It must name an amount. Bolt11Invoice is a string. |

**Returns.** `LnQuoteLike`. A frozen payment plan: the invoice amount, the `fee`, the route and the total.

**Throws.** [`AmountlessInvoice`](./errors.md#code-amountlessinvoice), [`NetworkMismatch`](./errors.md#code-networkmismatch), [`InvalidInput`](./errors.md#code-invalidinput), [`GatewayUnavailable`](./errors.md#code-gatewayunavailable), [`InsufficientBalance`](./errors.md#code-insufficientbalance), [`Recovering`](./errors.md#code-recovering), [`NotSupported`](./errors.md#code-notsupported), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`Internal`](./errors.md#code-internal), [`FederationClosed`](./errors.md#code-federationclosed). InvalidInput means an expired invoice.

**Example**
{#example}

```ts [example.ts]
import type { Bolt11Invoice, LightningLike } from '@fedimint/react-native'

declare const ln: LightningLike
declare const invoice: Bolt11Invoice

const quote = await ln.quote(invoice) // [!code focus]
console.log(quote.total())
```

**Used in.** [Send, step 04](./send.md#step-04)
