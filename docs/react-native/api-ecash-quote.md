---
title: 'quote()'
description: 'Plans a send of ecash.'
---

# EcashLike.quote

<!-- signature: EcashLike['quote'] -->

```ts
quote(amount: Amount): Promise<EcashQuoteLike>
```

Plans a send of ecash. Nothing is debited.

**Parameters**

| Name     | Type     | Description                                                                                                    |
| -------- | -------- | -------------------------------------------------------------------------------------------------------------- |
| `amount` | `Amount` | The least value the receiver must be able to redeem, in msat (a bigint). The real value is the same or higher. |

**Returns.** `EcashQuoteLike`. A frozen plan. Nothing is debited yet.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput), [`InsufficientBalance`](./errors.md#code-insufficientbalance), [`NotSupported`](./errors.md#code-notsupported), [`Recovering`](./errors.md#code-recovering), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`FederationClosed`](./errors.md#code-federationclosed). InvalidInput means a zero amount.

**Example**
{#example}

```ts [example.ts]
import type { EcashLike } from '@fedimint/react-native'

declare const ecash: EcashLike

const quote = await ecash.quote(5000n) // 5 sats, in msat // [!code focus]
console.log(quote.total())
```

**Used in.** [Send, step 01](./send.md#step-01)
