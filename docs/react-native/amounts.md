---
title: 'Amounts'
description: 'Units, conversion, and the amount types.'
---

# Amounts

Change units in three places of your own, and store amounts as plain numbers.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="bigint msat" /> <Badge type="danger" text="no helpers ship" /></p>

Every amount in this SDK is a `bigint` of millisatoshis. One sat is `1000n`. The SDK never rounds an amount for you: a fee of half a sat is `500n`, not zero. The type is called `Amount`, and every name that holds one should say its unit, `msat` or `sats`.

### 01 · Change units in exactly three places <Badge type="info" text="JavaScript" /> {#step-01}

Users think in sats. The SDK takes and returns msat. The arithmetic is short: `BigInt(sats) * 1000n` turns sats into msat, and `msat / 1000n` turns msat into whole sats, rounded down. The SDK ships no helpers for this, so write three of your own and change units only there.

```ts [amounts.ts]
// amounts.ts: your code. The SDK ships no amount helpers.
import type { Amount } from '@fedimint/react-native'

/** Whole sats to msat. Throws RangeError on a fraction: half a sat is 500n msat, so say so. */
export function satsToMsat(sats: number | bigint): Amount {
  return BigInt(sats) * 1000n // BigInt(12.5) throws RangeError
}

/** msat to whole sats, rounded down, as a number for a chart or your own storage. */
export function msatToSats(msat: Amount): number {
  return Number(msat / 1000n) // every real amount of bitcoin fits a safe integer of sats
}

/** Exact sats as text, for the screen: 1250500n is "1250.5". */
export function formatSats(msat: Amount): string {
  const fraction = (msat % 1000n).toString().padStart(3, '0').replace(/0+$/, '')
  return fraction ? `${msat / 1000n}.${fraction}` : `${msat / 1000n}`
}
```

| Your helper        | From, to                                     | Fractions                         |
| ------------------ | -------------------------------------------- | --------------------------------- |
| `satsToMsat(sats)` | whole sats, as `number` or `bigint`, to msat | Throws `RangeError` on a fraction |
| `formatSats(msat)` | msat to text, for the screen                 | Exact: `"1250.5"`                 |
| `msatToSats(msat)` | msat to a whole `number` of sats             | Rounds down                       |

```ts [use-amounts.ts]
import { formatSats, msatToSats, satsToMsat } from './amounts'

// Sats the user typed, into the msat every call takes.
export const msat = satsToMsat(1250) // 1250000n

// Exact, for the screen. A fraction of a sat is shown, not hidden.
export const shown = formatSats(1250500n) // "1250.5"

// Whole sats as a number, rounded DOWN, for a chart or your own storage.
export const whole = msatToSats(1250999n) // 1250
```

Use `formatSats` for anything a user reads, so that the screen and the wallet agree to the millisat. Use `msatToSats` only where a whole number is what you want and losing a fraction of a sat is fine.

::: tip Check
Log `shown` and `whole`. You will see `1250.5` and `1250`. Call `satsToMsat(12.5)` and you will see a `RangeError`.
:::

### 02 · Convert at the edge before you store anything <Badge type="info" text="JavaScript" /> {#step-02}

`JSON.stringify` throws on a `bigint`. So does anything built on it: AsyncStorage, MMKV with JSON values, a Redux persist layer, a network request body. The rule is simple. Never store an SDK object. At the edge of your app, turn it into a row of your own with plain `number` and `string` fields, and store that.

```ts [save-balance.ts]
import type { FederationLike } from '@fedimint/react-native'

/** Your own row: plain numbers and strings, so JSON.stringify can write it. */
export interface SavedBalance {
  federationId: string
  sats: number
  savedAt: number
}

export async function balanceToSave(fed: FederationLike): Promise<string> {
  const msat = await fed.balance()

  // JSON.stringify({ msat }) throws "TypeError: Do not know how to serialize a BigInt".
  // Convert at the edge instead, and store your own shape, never the SDK's.
  const row: SavedBalance = {
    federationId: fed.id(),
    sats: Number(msat / 1000n),
    savedAt: Date.now(),
  }
  return JSON.stringify(row)
}
```

::: warning
**A saved balance is a picture, not the balance.** Show it while the real one loads, then replace it with what `useBalance` reports. Never decide whether a send is affordable from a stored number.
:::

::: tip Check
Call `balanceToSave` on a federation you just joined. You will see a string such as `{"federationId":"…","sats":0,"savedAt":…}`. [The receive guide](./receive.md) puts money in, and the next save shows the new number.
:::

## Types {#types}

The SDK exports three number types from its package root. Each one is a `bigint`, and each has one unit.

| Type        | Unit                         | Where you meet it                                                                  |
| ----------- | ---------------------------- | ---------------------------------------------------------------------------------- |
| `Amount`    | Millisatoshis                | Every ecash and lightning amount, fee and total, and every balance.                |
| `Sats`      | Whole sats                   | On chain amounts, from `fed.onchain()`. Bitcoin on chain has no smaller unit.      |
| `Timestamp` | Milliseconds since 1970, UTC | Times in history rows and operation details. `new Date(Number(t))` makes a `Date`. |

## What you just did {#what-you-just-did}

You moved between msat and sats in the only three places a unit changes, and turned an amount into a row your app can store. [The history guide](./history.md) uses the same rule for history rows.
