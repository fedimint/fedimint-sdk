---
title: 'See the history'
description: 'Page through operations and turn each one into a row.'
---

# See the history

Page through operations and turn each one into a row.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="one list, both rails" /> <Badge type="tip" text="pages, newest first" /> <Badge type="tip" text="kept across restarts" /></p>

Every send and receive the wallet has made is kept in its data directory, across restarts. This page reads that record: as a list, one page at a time, and as a single operation you can open again and keep watching.

### 01 · Read the newest page <Badge type="info" text="JavaScript" /> {#step-01}

`fed.activity(cursor, limit)` returns a page of `ActivityItem` rows, newest first, for both rails in one list. Pass `undefined` as the cursor for the first page. `limit` is the most rows a page holds. It is required, and zero fails with `InvalidInput`.

```ts [first-page.ts]
import type { ActivityPage, FederationLike } from '@fedimint/react-native'

/** The newest twenty operations, both rails, in one list. */
export function firstPage(fed: FederationLike): Promise<ActivityPage> {
  return fed.activity(undefined, 20)
}
```

| Field           | Type                        | Means                                                              |
| --------------- | --------------------------- | ------------------------------------------------------------------ |
| `operationId`   | `string`                    | Opens the operation again, in step 04                              |
| `kind`          | `OperationKind`             | `EcashSend`, `EcashReceive`, `LnSend`, `LnReceive` and others      |
| `status`        | `ActivityStatus`            | How it turned out. Step 03                                         |
| `isFinal`       | `boolean`                   | Whether it can still change                                        |
| `amount`, `fee` | `bigint` msat, or undefined | The two numbers a list shows. `fee` is never folded into `amount`  |
| `direction`     | `Direction`, or undefined   | `Incoming` or `Outgoing`                                           |
| `time`          | `bigint`                    | Epoch milliseconds, from this device's clock, not the federation's |

You will see up to twenty rows, with the newest first. A new wallet shows an empty `items` array.

### 02 · Load the next page <Badge type="info" text="JavaScript" /> {#step-02}

A page carries `next`, a cursor for the page after it, or `undefined` on the last page. Pass it back as the first argument when the user scrolls to the end of the list.

```ts [more-pages.ts]
import type {
  ActivityItem,
  Cursor,
  FederationLike,
} from '@fedimint/react-native'

/** For a list that loads more as the user scrolls. */
export async function loadMore(
  fed: FederationLike,
  after: Cursor | undefined,
): Promise<{ items: ActivityItem[]; next: Cursor | undefined }> {
  // undefined for the first page. The previous page's next after that.
  const page = await fed.activity(after, 20)
  return { items: page.items, next: page.next }
}
```

`Cursor` is a plain string. You can keep it in state or in storage, and pass it back unchanged. Do not build one yourself: a cursor this federation did not issue fails with `InvalidInput`.

You will see the older rows. When `next` is `undefined`, the list has ended.

### 03 · Turn each row into your own, and know what pending means <Badge type="info" text="JavaScript" /> {#step-03}

An `ActivityItem` holds `bigint` values, so the rule from [Show a balance](./balance.md) applies: convert it at the edge into a row of your own before you keep it, cache it or send it anywhere.

```ts [to-row.ts]
import {
  ActivityStatus,
  Direction,
  OperationKind,
  type ActivityItem,
} from '@fedimint/react-native'

/** Your list row. Plain numbers and strings, so it can go through JSON and into storage. */
export interface HistoryRow {
  id: string
  kind: string // "EcashSend", "LnReceive", ...
  status: string // "Pending", "Success", "Refunded", ...
  incoming: boolean | null // null when the row has no direction
  sats: number | null // null when the row carries no amount
  feeSats: number | null // null when the fee is not known yet
  time: number // epoch milliseconds, from this device's clock
  spinner: boolean // true while the operation can still change
}

export function toRow(item: ActivityItem): HistoryRow {
  return {
    id: item.operationId,
    kind: OperationKind[item.kind],
    status: ActivityStatus[item.status],
    incoming:
      item.direction === undefined
        ? null
        : item.direction === Direction.Incoming,
    sats: item.amount === undefined ? null : Number(item.amount / 1000n),
    feeSats: item.fee === undefined ? null : Number(item.fee / 1000n),
    time: Number(item.time),
    spinner: !item.isFinal,
  }
}
```

| Status     | Means                                                                                                                                             |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Pending`  | Still moving. The only status that can change                                                                                                     |
| `Success`  | Completed as intended                                                                                                                             |
| `Refunded` | Did not complete, and the value is spendable again                                                                                                |
| `Canceled` | Called off or lapsed: ecash taken back, an invoice that expired unpaid. Nothing went wrong                                                        |
| `Failed`   | Did not complete, and the value is not known to be back. Worth a look                                                                             |
| `Unknown`  | A row this version of the SDK cannot read. It has no amount and no direction, so do not show it as money in or out. Use `isFinal` for the spinner |

You will see one plain row for each item, with names such as `"LnReceive"` and `"Success"`.

::: info
Decide whether to draw a spinner from `isFinal`, not from `status`. They agree on every row the SDK understands, and `isFinal` still answers correctly on a row it does not.
:::

### 04 · Open one operation again <Badge type="info" text="JavaScript" /> {#step-04}

A list row is two numbers and a status. A receipt screen wants more. `fed.operation(id)` finds the operation again, even after a restart, and its [`kind()`](./api-operation-kind.md) tells you which accessor to use: [`asEcashSend()`](./api-operation-asecashsend.md), `asEcashReceive()`, `asLnSend()` or `asLnReceive()`. Each returns the same kind of operation as the screen that started it, so [`updates()`](./api-operation-updates.md), [`awaitFinal()`](./api-operation-awaitfinal.md), `useOperation` and, for an ecash send, `requestCancel()` all work.

```ts [open-one.ts]
import {
  EcashSendState,
  OperationKind,
  type FederationLike,
} from '@fedimint/react-native'

/** A tap on a row: open that operation again and read what the SDK kept about it. */
export async function reopenEcashSend(
  fed: FederationLike,
  operationId: string,
) {
  const any = await fed.operation(operationId)
  if (!any) return null // not an operation of this federation
  if (any.kind() !== OperationKind.EcashSend) return null

  // Each accessor returns undefined unless the operation is of that kind.
  const send = any.asEcashSend()
  if (!send) return null

  const state = await send.state()
  const details = await send.details()

  return {
    redeemed: state === EcashSendState.Redeemed,
    // Still the same bearer string. Show it again only to the person who sent it.
    notes: details.notes.display(),
    // After this time the wallet takes unredeemed notes back.
    reclaimAt: new Date(Number(details.reclaimAt)),
    // The same operation as on the send screen: updates, awaitFinal and requestCancel all work.
    operation: send,
  }
}
```

[`details()`](./api-operation-details.md) is what the SDK recorded about the operation. For an ecash send, that is the notes, the amounts, the fee and when unredeemed notes come back. For a Lightning send, it is the invoice, the route and the fee. Read it for a receipt rather than rebuilding one from list rows.

You will see the send's notes and reclaim time again, for an operation made before the app restarted.

## What you just did {#what-you-just-did}

You listed both rails in one history, page by page, turned each row into a plain one your app can store, drew spinners from the one field that is always right, and opened an old operation again to read its receipt and keep watching it.
