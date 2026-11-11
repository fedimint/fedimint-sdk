---
title: 'Show a balance'
description: 'Read the balance once, then follow it as it changes.'
---

# Show a balance

Read the balance once, then follow it as it changes.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="first value is the balance now" /> <Badge type="tip" text="useBalance ships in the package" /> <Badge type="danger" text="no retry after FederationClosed" /></p>

Every amount in this SDK is a `bigint` of millisatoshis. One sat is `1000n`. The core never rounds an amount for you, so a fee of half a sat is `500n`, not zero. The type is called `Amount`. No conversion helpers ship, so do the arithmetic inline: `msat / 1000n` gives whole sats.

That choice costs you one rule, and [Amounts](./amounts.md) ends with it: a `bigint` cannot go through `JSON.stringify`.

### 01 · Read it once <Badge type="info" text="JavaScript" /> {#step-01}

[`fed.balance()`](./api-federation-balance.md) is the spendable balance of one federation, now. Use it where you need a number once, such as before a send.

```ts [read-balance.ts]
import type { FederationLike } from '@fedimint/react-native'

export async function balanceLine(fed: FederationLike): Promise<string> {
  // A bigint, in millisatoshis: 1000n is one sat. A new wallet answers 0n.
  const msat = await fed.balance()
  return `${msat / 1000n} sats`
}
```

::: tip Check
On a federation you just joined, you will see `0 sats`.
:::

### 02 · Follow it <Badge type="info" text="JavaScript" /> {#step-02}

A screen that shows a balance wants every change, not one reading. [`fed.balanceUpdates()`](./api-federation-balanceupdates.md) returns a cursor. Its first `next()` returns the balance now. Each later `next()` waits for the next change. Pass an `AbortSignal` so that you can stop it. This is how code outside React follows a balance:

```ts [follow-balance.ts]
import type { Amount, FederationLike } from '@fedimint/react-native'

/** Calls onChange with the balance now, then on every change. Returns a function that stops it. */
export function followBalance(
  fed: FederationLike,
  onChange: (msat: Amount) => void,
  onError: (err: unknown) => void,
): () => void {
  const controller = new AbortController()
  const { signal } = controller
  const updates = fed.balanceUpdates()

  void (async () => {
    try {
      for (;;) {
        // The first value is the balance now. Every later value is a change.
        const msat = await updates.next({ signal })
        if (signal.aborted) return
        onChange(msat)
      }
    } catch (err) {
      // A balance never ends cleanly. FederationClosed is how the stream stops.
      if (!signal.aborted) onError(err)
    }
  })()

  return () => controller.abort()
}
```

In React, use `useBalance`. It does the same work, and it stops when the component unmounts:

```tsx [BalanceLine.tsx]
import React from 'react'
import { Text } from 'react-native'
import { useBalance, type FederationLike } from '@fedimint/react-native'

export function BalanceLine({ fed }: { fed: FederationLike | undefined }) {
  // undefined until the first value arrives, then the balance in msat, kept current.
  const msat = useBalance(fed, (err) => console.log('balance watch', err))
  return (
    <Text>{msat === undefined ? 'loading...' : `${msat / 1000n} sats`}</Text>
  )
}
```

::: info
The `onError` callback hears about failures to watch, not failures of money. The usual one is `FederationClosed`, after the federation or the SDK closes. Neither `followBalance` nor `useBalance` starts again by itself. After you reopen the federation, start a new watch. For `useBalance`, remount the component with a new React `key`. [Share the session](./session.md) shows how.
:::

::: tip Check
Render `BalanceLine` and receive some ecash as in [Receive money](./receive.md). You will see the line change from `0 sats` to the new balance with no code of yours in between.
:::

## What you just did {#what-you-just-did}

You read a balance once, and followed it as it changes, both outside React and with `useBalance`. [See the history](./history.md) uses the same `bigint` amounts for history rows.
