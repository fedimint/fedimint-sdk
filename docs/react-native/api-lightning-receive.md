---
title: 'receive()'
description: 'Creates an invoice that the wallet can get paid on.'
---

# LightningLike.receive

<!-- signature: LightningLike['receive'] -->

```ts
receive(amount: Amount, description: string): Promise<LnReceiveHandle>
```

Creates an invoice that the wallet can get paid on.

**Parameters**

| Name          | Type     | Description                                |
| ------------- | -------- | ------------------------------------------ |
| `amount`      | `Amount` | The amount to ask for, in msat (a bigint). |
| `description` | `string` | The text inside the invoice.               |

**Returns.** `LnReceiveHandle`. An object with the invoice to show the payer and the operation to follow.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput), [`GatewayUnavailable`](./errors.md#code-gatewayunavailable), [`Recovering`](./errors.md#code-recovering), [`NotSupported`](./errors.md#code-notsupported), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`Storage`](./errors.md#code-storage), [`Internal`](./errors.md#code-internal), [`FederationClosed`](./errors.md#code-federationclosed). InvalidInput means a zero amount, a bad description, or an amount too small for the fee.

**Example**
{#example}

```ts [example.ts]
import type { LightningLike } from '@fedimint/react-native'

declare const ln: LightningLike

const { invoice, operation } = await ln.receive(5000n, 'Coffee') // 5 sats, in msat // [!code focus]
console.log(invoice, operation !== undefined)
```

**Used in.** [Receive, step 03](./receive.md#step-03)
