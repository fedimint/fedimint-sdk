---
title: 'balance()'
description: 'Reads the balance of the federation once.'
---

# FederationLike.balance

<!-- signature: FederationLike['balance'] -->

```ts
balance(): Promise<Amount>
```

Reads the balance of the `federation` once.

**Parameters.** None.

**Returns.** `Amount`. The notes the wallet holds as its own, in msat (a bigint). Money inside an operation that is still running is not counted. During a recovery the value is partial, and none of it is spendable.

**Throws.** [`Internal`](./errors.md#code-internal), [`FederationClosed`](./errors.md#code-federationclosed).

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const msat = await fed.balance() // [!code focus]
console.log(`${msat / 1000n} sats`)
```

**Used in.** [Balance, step 01](./balance.md#step-01), [Amounts, step 02](./amounts.md#step-02)
