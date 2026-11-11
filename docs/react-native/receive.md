---
title: 'Receive money'
description: 'Redeem ecash, and create and watch a Lightning invoice.'
---

# Receive money

Redeem ecash, and create and watch a Lightning invoice.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="ecash in" /> <Badge type="tip" text="Lightning in" /> <Badge type="tip" text="useOperation hook included" /> <Badge type="danger" text="on chain deposits not covered here" /></p>

Money arrives two ways. Ecash comes as a string somebody hands you, in a chat, a QR code or a file. Lightning comes as a payment to an invoice you create. Both give you back an operation: a handle on money that is on its way, which you can watch until it lands.

Each rail is a method on the federation, [`fed.ecash()`](./api-federation-ecash.md) and [`fed.lightning()`](./api-federation-lightning.md). Either can return `undefined`, because a federation that does not run that module cannot offer it. The types make you check, so a missing module is a message on screen and not a crash.

```text no-check
operation.updates()     a cursor. next() gives the state now, then each change, then undefined
operation.awaitFinal()  resolves with the final state: success, failure or expiry alike
operation.state()       the state now, once
operation.id()          a string, to find it again in history (See the history)
useOperation(op)        the same states in a React component. It stops when the component unmounts
```

### 01 · Ecash: find out what it is worth <Badge type="info" text="JavaScript" /> {#step-01}

Before the user accepts a string, tell them what it is worth. `Notes.parse(str).value()` reads the value written inside the notes, in msat. It is instant and talks to no network.

```ts [inspect-notes.ts]
import { Notes, type FederationLike } from '@fedimint/react-native'

/** What a pasted ecash string is worth, before the user commits to it. No network. */
export function describeNotes(fed: FederationLike, pasted: string): string {
  // Undefined when this federation does not run the ecash module. The compiler makes you check.
  if (!fed.ecash()) return 'This federation does not take ecash.'

  // Throws InvalidInput when the string is not ecash.
  const msat = Notes.parse(pasted.trim()).value()
  return `${msat / 1000n} sats`
}
```

You will see the value of the notes in sats, before anything is redeemed.

::: info
`value()` reads the notes. It cannot know whether they were already spent or taken back by the sender. Only the receive in step 02 finds that out.
:::

### 02 · Ecash: redeem it <Badge type="info" text="JavaScript" /> {#step-02}

`ecash.receive(Notes.parse(str))` exchanges the notes with the federation for fresh ones that belong to this wallet. Only then is the money yours. Until that happens the sender can still take it back. So redeem as soon as the user says yes.

```ts [redeem-notes.ts]
import {
  EcashReceiveState_Tags,
  Notes,
  type EcashReceiveState,
  type FederationLike,
} from '@fedimint/react-native'

export async function redeem(
  fed: FederationLike,
  pasted: string,
): Promise<EcashReceiveState> {
  const ecash = fed.ecash()
  if (!ecash) throw new Error('This federation does not take ecash.')

  // Returns as soon as the redemption has started. The notes must come from this
  // federation: notes from another one fail here with InvalidInput.
  const operation = await ecash.receive(Notes.parse(pasted.trim()))

  // Show progress while you wait. The first value is the state now.
  const updates = operation.updates()
  for (;;) {
    const state = await updates.next()
    if (state === undefined) break // the stream has ended
    if (state.tag === EcashReceiveState_Tags.Issuing)
      console.log('redeeming...')
  }

  // awaitFinal() resolves with the final state, good or bad. It does not throw for a failed receive.
  const final = await operation.awaitFinal()
  if (final.tag === EcashReceiveState_Tags.Failed) {
    // Most often the sender took the notes back first.
    console.log('receive failed:', final.inner.reason)
  }
  return final // Done means the value is in the balance and spendable.
}
```

| State     | Means                                                   |
| --------- | ------------------------------------------------------- |
| `Created` | Accepted here, about to go to the federation            |
| `Issuing` | The federation is issuing fresh notes                   |
| `Done`    | Final. The value is in the balance and can be spent     |
| `Failed`  | Final. Nothing arrived. `inner.reason` is for your logs |

You will see the final state. After `Done`, the balance shows the new value.

::: info
Any fee comes out of the notes, not on top. There is no quote on this side, because the receiver has nothing to decide.
:::

### 03 · Lightning: create an invoice <Badge type="info" text="JavaScript" /> {#step-03}

`ln.receive(msat, description)` asks a Lightning gateway for an invoice and returns it with the operation that will see it paid. The amount is a msat `bigint`, so multiply what the user typed in sats by `1000n`. The description is required. The payer's wallet shows it.

```ts [create-invoice.ts]
import { Network, type FederationLike } from '@fedimint/react-native'

/** Set this to true only in a build that is meant to hold real money. */
const REAL_MONEY_BUILD = false

export async function invoiceFor(fed: FederationLike, sats: number) {
  const ln = fed.lightning()
  if (!ln) throw new Error('This federation has no Lightning module.')

  // A test build asks for test money only. Signet, like Fedi Testnet, is safe to play with.
  if (fed.network() === Network.Bitcoin && !REAL_MONEY_BUILD) {
    throw new Error(
      'This federation is on Bitcoin mainnet. Refusing in a test build.',
    )
  }

  // The amount is in msat. The description is required, and the payer sees it.
  const { invoice, operation } = await ln.receive(
    BigInt(sats) * 1000n,
    'Coffee',
  )
  return { invoice, operation } // show invoice as text and as a QR code
}
```

You will see an invoice string that starts with `lntbs` on signet.

::: warning
**Check the network before you ask for money.** While you build, use a signet federation such as Fedi Testnet and pay the invoice from a signet faucet. The check above stops a test build from asking anybody for real bitcoin because a mainnet invite slipped into a config file.
:::

### 04 · Lightning: watch the invoice <Badge type="info" text="JavaScript" /> {#step-04}

The state is a tagged union. Switch on `state.tag`. The compiler knows every case, offers them as you type, and lets you read `state.inner` only on the cases that carry one. `Failed` here is not an expired invoice. It means a payment arrived and did not become money in the wallet, so show it as an error to report.

Outside React, pull the [`updates()`](./api-operation-updates.md) cursor. Pass an `AbortSignal`, and abort it when the screen closes, so nothing runs on a screen that has gone.

```ts [watch-invoice.ts]
import {
  LnReceiveState_Tags,
  type LnReceiveOperationLike,
  type LnReceiveState,
} from '@fedimint/react-native'

/** What to show for each state. */
export function describeInvoice(state: LnReceiveState): string {
  switch (state.tag) {
    case LnReceiveState_Tags.Created:
    case LnReceiveState_Tags.WaitingForPayment:
      return 'Waiting for payment'
    case LnReceiveState_Tags.Funded:
      return 'Paid. Claiming...'
    case LnReceiveState_Tags.Claimed:
      return 'Received' // in the balance now
    case LnReceiveState_Tags.Expired:
      return 'This invoice expired unpaid'
    case LnReceiveState_Tags.Canceled:
      console.log('invoice canceled:', state.inner.reason) // for your logs, not the screen
      return 'The gateway withdrew this invoice. Make a new one'
    case LnReceiveState_Tags.Failed:
      return 'Something went wrong with this payment. Please report it'
  }
}

/** Follows the invoice until it ends, or until signal is aborted. */
export async function watchInvoice(
  operation: LnReceiveOperationLike,
  show: (text: string) => void,
  signal: AbortSignal,
): Promise<void> {
  const updates = operation.updates()
  try {
    for (;;) {
      // The first value is the state now. undefined means the stream has ended.
      const state = await updates.next({ signal })
      if (state === undefined) return
      show(describeInvoice(state))
    }
  } catch (err) {
    if (!signal.aborted) throw err // an abort is the screen closing, not an error
  }
}
```

In a React screen, `useOperation` does the same work. It starts on mount and stops on unmount. It returns `undefined` until the first state arrives, and keeps the final state after the stream ends.

```ts [InvoiceStatus.tsx]
import { Text } from 'react-native';
import { useOperation, type LnReceiveOperationLike } from '@fedimint/react-native';
import { describeInvoice } from './watch-invoice';

export function InvoiceStatus({ operation }: { operation: LnReceiveOperationLike }) {
  // A failure to watch goes to the second argument. It is never the payment's outcome.
  const state = useOperation(operation, err => console.log('cannot watch the invoice:', err));
  return <Text>{state === undefined ? 'Loading...' : describeInvoice(state)}</Text>;
}
```

::: tip Check
Pay the invoice from a signet faucet. You will see the state move to `Claimed`, and the balance line from [Show a balance](./balance.md) changes by itself.
:::

## What you just did {#what-you-just-did}

You priced a string of ecash before accepting it, redeemed it, created a Lightning invoice after checking the network, and followed both operations to a final state.

This page does not cover on chain deposits. They live on [`fed.onchain()`](./api-federation-onchain.md), which can return `undefined` like the other two rails.
