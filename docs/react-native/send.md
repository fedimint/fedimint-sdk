---
title: 'Send money'
description: 'Quote and send ecash, take it back, and pay a Lightning invoice.'
---

# Send money

Quote and send ecash, take it back, and pay a Lightning invoice.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="quote, then send" /> <Badge type="tip" text="ecash can be taken back" /> <Badge type="danger" text="the sender learns of a redemption only at reclaim" /> <Badge type="danger" text="a Lightning payment cannot be cancelled" /> <Badge type="danger" text="on chain sends not covered here" /></p>

Sending is where a wallet can lose somebody's money, so this page is more careful than the last one. Both rails work the same way, in two calls. First a **quote**: the SDK works out exactly what the send will cost and freezes it. You show those numbers and the user confirms. Then **send**, with that quote and nothing else. The user never agrees to one number and pays another.

```text no-check
ecash      quote(msat)     -> show fee and total -> send(quote) -> { notes, operation }
lightning  quote(invoice)  -> show fee and total -> send(quote) -> operation
```

Two more rules carry the rest. Ecash leaves the balance before anybody has received it. And a Lightning send that returns has started, not finished.

### 01 · Ecash: quote it <Badge type="info" text="JavaScript" /> {#step-01}

A federation issues notes in fixed sizes, so the notes for a request can be worth a little more than was asked. The quote shows both, and the fee, and the total the balance will move by. Every amount is a msat `bigint`.

```ts [quote-ecash.ts]
import type { EcashQuoteLike, FederationLike } from '@fedimint/react-native'

/** Prices a send. Nothing leaves the balance yet. */
export async function priceEcash(fed: FederationLike, sats: number) {
  const ecash = fed.ecash()
  if (!ecash) throw new Error('This federation does not take ecash.')
  const quote: EcashQuoteLike = await ecash.quote(BigInt(sats) * 1000n)

  // What the confirm screen shows. total() is what the balance moves by.
  const lines = {
    asked: `${quote.requestedAmount() / 1000n} sats`,
    notesWorth: `${quote.notesValue() / 1000n} sats`, // can be a little more than asked
    fee: `${quote.fee() / 1000n} sats`,
    total: `${quote.total() / 1000n} sats`,
  }
  return { quote, lines }
}
```

| On the quote                                          | Means                                                             |
| ----------------------------------------------------- | ----------------------------------------------------------------- |
| [`requestedAmount()`](./api-ecash-requestedamount.md) | What you asked for                                                |
| [`notesValue()`](./api-ecash-notesvalue.md)           | What the receiver can redeem. Never less than asked               |
| `fee()`                                               | The cost on top. Show it, even when it is zero                    |
| `total()`                                             | What leaves the balance. Show this one before the user confirms   |
| `expiresAt()`                                         | When the quote stops working, as a `bigint` of epoch milliseconds |

You will see the four numbers in sats. The balance has not changed.

::: info
A quote can fail with `NotSupported` when the wallet's notes cannot make up the amount. This SDK hands out only notes it already holds, and does not make change. Say so, and suggest a different amount. A quote can also fail with `InsufficientBalance`, even for an amount the balance would cover exactly, because the notes round up.
:::

### 02 · Ecash: hand over a string <Badge type="info" text="JavaScript" /> {#step-02}

Sending takes the total out of the balance and gives you notes. [`notes.display()`](./api-values-notes-display.md) turns them into the string to hand over. Whoever holds that string holds the money. There is no recipient, no address and no confirmation.

```ts [send-ecash.ts]
import {
  ErrorCode,
  Exception,
  type EcashQuoteLike,
  type FederationLike,
} from '@fedimint/react-native'

/** After the user tapped Confirm on the quote. Returns the string to hand over. */
export async function sendEcash(fed: FederationLike, quote: EcashQuoteLike) {
  const ecash = fed.ecash()
  if (!ecash) throw new Error('This federation does not take ecash.')
  try {
    // The balance drops when this returns, before anybody has redeemed anything.
    const { notes, operation } = await ecash.send(quote)
    return { text: notes.display(), operation }
  } catch (err) {
    const code = Exception.hasInner(err)
      ? Exception.getInner(err).code()
      : undefined
    if (code === ErrorCode.QuoteExpired || code === ErrorCode.QuoteChanged) {
      // The quote cannot be used. Quote again and ask the user to confirm the new numbers.
      return null
    }
    throw err
  }
}
```

You will see a long string, and the balance drops by `total()`.

::: warning
**Treat the string as cash on screen.** It is bearer money in plain text. Anything that copies it, screenshots it, logs it or syncs it is moving the money itself.

It is also often too long for one QR code. Give the user a copy button as well as a QR, and handle the QR library's failure rather than showing an empty square.
:::

A quote is for one send. After `expiresAt()` the send fails with `QuoteExpired`. If another send spent the notes it planned to use first, it fails with `QuoteChanged`. The remedy for both is the same: quote again and ask the user to confirm the new numbers.

### 03 · Ecash: take it back <Badge type="info" text="JavaScript" /> {#step-03}

Until the receiver redeems the string, the sender can ask for the value back. [`operation.requestCancel()`](./api-operation-requestcancel.md) records the request and returns. It does not wait for the network. The outcome arrives as a state, because the receiver may redeem first. `EcashSendState` is a plain enum, so compare it with `===`.

```ts [take-it-back.ts]
import {
  EcashSendState,
  type EcashSendOperationLike,
} from '@fedimint/react-native'

/** What to show for each state of a send. */
export function describeSend(state: EcashSendState): string {
  switch (state) {
    case EcashSendState.Created:
      return 'Sent. The receiver may have it already'
    case EcashSendState.CancelRequested:
      return 'Taking it back...'
    case EcashSendState.Canceled:
      return 'Back in your balance'
    case EcashSendState.Redeemed:
      return 'The receiver has it'
  }
}

/** The user changed their mind. Only an ecash send can be taken back. */
export async function takeItBack(
  operation: EcashSendOperationLike,
): Promise<string> {
  // Returns once the request is recorded. It can still lose the race to a
  // receiver who redeemed first.
  await operation.requestCancel()
  const final = await operation.awaitFinal()
  return final === EcashSendState.Canceled
    ? 'Back in your balance'
    : 'The receiver redeemed it first'
}
```

| State             | Means                                                                                                                                                        |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Created`         | The string exists. The receiver may have redeemed it already. The sender cannot see that yet, so do not wait for `Redeemed` to tell the user the send worked |
| `CancelRequested` | Taking it back. Not final: the receiver can still win                                                                                                        |
| `Canceled`        | Final. The value is back in the balance                                                                                                                      |
| `Redeemed`        | Final. The receiver has it                                                                                                                                   |

You will see `Back in your balance`, or `The receiver redeemed it first` if the receiver was faster.

::: info
**The sender learns the outcome only at reclaim.** A redemption sends nothing back to the sender. The SDK finds out only when it tries to take the notes back: after `requestCancel()`, or at the time in `details().reclaimAt`, when it takes unredeemed notes back by itself. On some federations that automatic reclaim runs only when your app reads the send again, so read its state after `reclaimAt`. [See the history](./history.md) shows how to open a send again.
:::

Only an ecash send has `requestCancel()`. The other operation types do not have the method, so the compiler stops you before you try.

### 04 · Lightning: quote the invoice <Badge type="info" text="JavaScript" /> {#step-04}

`ln.quote(invoice)` reads the invoice, picks a gateway and works out the fee. Most of what can go wrong with a payment goes wrong here, before any money moves, which is the best place for it.

```ts [quote-invoice.ts]
import {
  ErrorCode,
  Exception,
  type FederationLike,
  type LnQuoteLike,
} from '@fedimint/react-native'

export async function priceInvoice(fed: FederationLike, pasted: string) {
  const ln = fed.lightning()
  if (!ln) throw new Error('This federation has no Lightning module.')
  // Remove spaces and a "lightning:" prefix from what the user pasted or scanned.
  const invoice = pasted.trim().replace(/^lightning:/i, '')
  try {
    const quote: LnQuoteLike = await ln.quote(invoice)
    return {
      quote,
      pays: `${quote.invoiceAmount() / 1000n} sats`,
      fee: `${quote.fee() / 1000n} sats`,
      total: `${quote.total() / 1000n} sats`, // what the balance moves by
    }
  } catch (err) {
    const code = Exception.hasInner(err)
      ? Exception.getInner(err).code()
      : undefined
    if (code === ErrorCode.AmountlessInvoice)
      return 'Ask for an invoice with an amount.'
    if (code === ErrorCode.NetworkMismatch)
      return 'That invoice is for a different network.'
    if (code === ErrorCode.InsufficientBalance)
      return 'Not enough in this wallet.'
    if (code === ErrorCode.GatewayUnavailable)
      return 'No Lightning gateway answered. Try later.'
    throw err
  }
}
```

| Code                  | Means                                                                                |
| --------------------- | ------------------------------------------------------------------------------------ |
| `AmountlessInvoice`   | The invoice names no amount. This SDK cannot pay those                               |
| `NetworkMismatch`     | The invoice is for another network, such as a mainnet invoice on a signet federation |
| `InvalidInput`        | Not an invoice, or already expired                                                   |
| `InsufficientBalance` | The balance cannot cover `total()`                                                   |
| `GatewayUnavailable`  | No gateway could be reached and checked                                              |

You will see the invoice amount, the fee and the total in sats, or one of the messages above.

### 05 · Lightning: pay it, then wait for the answer <Badge type="info" text="JavaScript" /> {#step-05}

`ln.send(quote)` returns an operation as soon as the payment has started. The payment is not made yet. The final state is the answer, and only `Success` means the invoice was paid. A failed payment is a state, not a thrown error.

```ts [pay-invoice.ts]
import {
  LnSendState_Tags,
  type FederationLike,
  type LnQuoteLike,
} from '@fedimint/react-native'

export async function payInvoice(
  fed: FederationLike,
  quote: LnQuoteLike,
): Promise<string> {
  const ln = fed.lightning()
  if (!ln) throw new Error('This federation has no Lightning module.')

  // Returns when the payment has STARTED. Nothing is paid yet.
  const operation = await ln.send(quote)

  // The final state is the answer. awaitFinal() resolves for success, refund and failure alike.
  const final = await operation.awaitFinal()
  switch (final.tag) {
    case LnSendState_Tags.Success:
      // The preimage is the receipt: proof the invoice was paid.
      return `Paid. Fee ${final.inner.fee / 1000n} sats. Proof ${final.inner.preimage}`
    case LnSendState_Tags.Refunded:
      return 'Not paid. The money is back in your balance.'
    case LnSendState_Tags.Failed:
      // The SDK cannot say where the money is. Log the reason, then read the balance.
      console.log('payment failed:', final.inner.reason)
      return 'Not paid. Check your balance before you try again.'
    default:
      return 'Still in progress' // awaitFinal() only resolves on a final state
  }
}
```

| Final state | Means                                                         | Tell the user                         |
| ----------- | ------------------------------------------------------------- | ------------------------------------- |
| `Success`   | Paid. `inner.preimage` proves it, `inner.fee` is what it cost | Paid                                  |
| `Refunded`  | Not paid, and the money is spendable again                    | Not paid, nothing lost                |
| `Failed`    | Not paid, and the SDK cannot say where the money is           | Check the balance before trying again |

You will see `Paid` with the fee and the preimage, or one of the two other answers.

::: warning
**A Lightning payment cannot be cancelled.** Once sent it runs to one of those three endings. Do not offer a cancel button on this screen.
:::

## What you just did {#what-you-just-did}

You quoted before every send, so the user saw the fee and the total before agreeing. You handed out ecash and took it back, and you paid an invoice and waited for the answer instead of trusting the return of a call.

This page does not cover on chain sends. They live on [`fed.onchain()`](./api-federation-onchain.md), which can return `undefined` like the other two rails.
