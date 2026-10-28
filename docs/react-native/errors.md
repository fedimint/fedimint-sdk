---
title: 'Errors'
description: 'Exception and its codes, and how to explain a failure.'
---

# Errors

Read a failure by its code, and look up what each code means.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="23 stable codes" /> <Badge type="danger" text="not instanceof" /></p>

Every failure the SDK reports carries an `Exception`. Branch on its `code()`, log its `reason()`, and never show or match on `reason()`.

### 01 · Read a failure by its code <Badge type="info" text="JavaScript" /> {#step-01}

The SDK does not throw an `Exception` object itself. It throws a wrapper, so `err instanceof Exception` is `false`. Ask `Exception.hasInner(err)` first. If it is `true`, `Exception.getInner(err)` gives you the `Exception`, with three methods. Each has a different job.

| Method      | What it is                                                                                                    | What to do with it                                                   |
| ----------- | ------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `code()`    | An `ErrorCode` value, such as `ErrorCode.Timeout`. `ErrorCode[code]` is its name as text, such as `"Timeout"` | Branch on it. It is stable                                           |
| `reason()`  | The SDK's own sentence about what went wrong                                                                  | Log it. Never show it, and never match on it: its wording can change |
| `details()` | `undefined`, or `{ version, kind, payload }`. `kind` names the detail, such as `"StorageInUse"`               | Log `kind`. Do not show it                                           |

The SDK ships no helper for this. Put these two small functions in your app, and use them everywhere you catch an error.

```ts [errors.ts]
// errors.ts: your code. The SDK ships no helper for this.
import { ErrorCode, Exception } from '@fedimint/react-native'

/** The SDK's code for a failure, or undefined for an error that did not come from the SDK. */
export function errorCode(err: unknown): ErrorCode | undefined {
  return Exception.hasInner(err) ? Exception.getInner(err).code() : undefined
}

/** One line for your logs. Never show it to the user. */
export function explain(err: unknown): string {
  if (!Exception.hasInner(err)) return String(err)
  const e = Exception.getInner(err)
  const kind = e.details()?.kind
  return `${ErrorCode[e.code()]}: ${e.reason()}${kind ? ` (${kind})` : ''}`
}
```

Then branch on the code where you catch:

```ts [explain-failure.ts]
import { ErrorCode, InviteCode, type SdkLike } from '@fedimint/react-native'
import { errorCode, explain } from './errors'

export async function previewOrExplain(
  sdk: SdkLike,
  invite: string,
): Promise<string> {
  try {
    const preview = await sdk.preview(InviteCode.parse(invite.trim()))
    return preview.name ?? 'a federation that does not publish a name'
  } catch (err) {
    const code = errorCode(err)
    // No code means the error did not come from the SDK. Let it crash loudly.
    if (code === undefined) throw err

    // For you, not for the user: reason is free text and its wording can change.
    console.log(`preview failed: ${explain(err)}`)

    // For the user: branch on code, which is stable.
    switch (code) {
      case ErrorCode.InvalidInput:
        return 'That is not an invite code.'
      case ErrorCode.FederationUnreachable:
      case ErrorCode.Timeout:
        return 'Could not reach that federation. Check your connection and try again.'
      case ErrorCode.UnsupportedFederation:
        return 'This wallet cannot use that federation.'
      default:
        return 'Something went wrong.'
    }
  }
}
```

The codes you will meet first, and where:

| Code                               | Means                                                                     | Usually from                                                                                         |
| ---------------------------------- | ------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `InvalidInput`                     | A string that is not what it claims: invite, notes, invoice or seed words | The [join](./join.md), [receive](./receive.md), [send](./send.md) and [restore](./restore.md) guides |
| `FederationUnreachable`, `Timeout` | No guardian answered, or not in time                                      | Any call that talks to a federation                                                                  |
| `AlreadyJoined`                    | This wallet has joined that federation before                             | [The join guide](./join.md). Use `sdk.federation(id)` or `sdk.reopenFederation(id)`                  |
| `InsufficientBalance`              | Not enough to cover the total                                             | [The send guide](./send.md)                                                                          |
| `QuoteExpired`, `QuoteChanged`     | The quote cannot be used any more. Nothing was spent                      | [The send guide](./send.md). Quote again                                                             |
| `Recovering`                       | A restore has not reached `Done`, so money cannot move yet                | [The restore guide](./restore.md)                                                                    |
| `StorageInUse`                     | The wallet folder is already open                                         | Step 01 of [the seed words page](./seed-words.md)                                                    |
| `SeedMismatch`                     | The folder holds a different seed from the words you passed               | [The restore guide](./restore.md)                                                                    |
| `FederationClosed`                 | The federation or the whole SDK was closed                                | Any call after `session.close()`                                                                     |

::: info
The SDK keeps its own time limits. A preview or a join whose guardians do not answer in time rejects with `Timeout` or `FederationUnreachable`, so you do not need a timer of your own around those two calls.
:::

::: tip Check
Call `previewOrExplain(sdk, 'hello')`. You will see `That is not an invite code.`, and a log line that starts with `preview failed: InvalidInput:`.
:::

## Codes {#codes}

Every member of `ErrorCode`, with the meaning the SDK gives it. Compare with `===`: it is a plain enum.

| Code                                                                 | Means                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| <span id="code-invalidinput"></span>`InvalidInput`                   | The input could not be parsed or was structurally invalid: a malformed invite code, invoice, ecash notes, address, mnemonic or activity cursor.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| <span id="code-alreadyjoined"></span>`AlreadyJoined`                 | The federation identified by an invite code has already been joined.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| <span id="code-unsupportedfederation"></span>`UnsupportedFederation` | The federation cannot be used as configured. This includes mixed module generations within one federation (all modules must share the same v1 or v2 generation), and configurations the SDK otherwise refuses to operate on.                                                                                                                                                                                                                                                                                                                                                                                                    |
| <span id="code-federationunreachable"></span>`FederationUnreachable` | No guardian could be reached to service the request.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| <span id="code-insufficientbalance"></span>`InsufficientBalance`     | The spendable balance is too low to cover the requested amount.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| <span id="code-balancenotempty"></span>`BalanceNotEmpty`             | A request to permanently forget a federation was made while spendable balance still remains in it.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| <span id="code-pendingoperations"></span>`PendingOperations`         | A request to permanently forget a federation was made while non-final operations or reclaimable outgoing value still exist for it. An incomplete recovery is deliberately not one of those reasons, so this code is never returned on that account.                                                                                                                                                                                                                                                                                                                                                                             |
| <span id="code-gatewayunavailable"></span>`GatewayUnavailable`       | No usable lightning gateway is currently available.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| <span id="code-recovering"></span>`Recovering`                       | The requested action is unavailable because this federation's recovery is incomplete. Incomplete is not the same as still running: a recovery that stopped short leaves the lock in place, and only a recovery that runs to completion releases it.                                                                                                                                                                                                                                                                                                                                                                             |
| <span id="code-notsupported"></span>`NotSupported`                   | The federation does not have the module backing this facade. This occurs when a facade obtained earlier is used after the federation's configuration changed to drop that module.                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| <span id="code-unsupportedoperation"></span>`UnsupportedOperation`   | A persisted operation exists but this build cannot interpret it: the record names a kind this build does not know, or a state schema newer than it reads. The operation is still observable but not actionable.                                                                                                                                                                                                                                                                                                                                                                                                                 |
| <span id="code-seedmismatch"></span>`SeedMismatch`                   | Storage already holds a seed and it does not match the mnemonic supplied to open it.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| <span id="code-storageinuse"></span>`StorageInUse`                   | The storage location is already open, in this process or another.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| <span id="code-storageorphaned"></span>`StorageOrphaned`             | Storage holds federation or client state but no usable seed, so it cannot be opened. The seed entry is either absent, or present and unusable. This is permanent, not transient: asking again does not change it, and the refusal changes nothing in the store. Do not retry and do not repair it automatically. Report it and offer the ways out, in this order: update the SDK or the app where the entry was present but unusable; point at a different location; or, as a last resort a person chooses explicitly, abandon the location by deleting its contents, which destroys any funds whose only backup was that seed. |
| <span id="code-quoteexpired"></span>`QuoteExpired`                   | The quote passed to `send` is no longer valid: either its validity window has passed, or it has already been executed, and a quote funds exactly one payment. Both have the same remedy: obtain a fresh quote and retry.                                                                                                                                                                                                                                                                                                                                                                                                        |
| <span id="code-quotechanged"></span>`QuoteChanged`                   | Conditions material to the quote (fees, routing, federation state) changed since it was issued. Obtain a fresh quote and retry.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| <span id="code-amountlessinvoice"></span>`AmountlessInvoice`         | The bolt11 invoice specifies no amount, and such an invoice cannot be paid. No amount the caller supplies can make it payable, because Fedimint does not support paying amountless bolt11 invoices at all. The only remedy is a different invoice, one that names its own amount.                                                                                                                                                                                                                                                                                                                                               |
| <span id="code-networkmismatch"></span>`NetworkMismatch`             | A supplied address or invoice is for a different network than the federation's.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| <span id="code-federationclosed"></span>`FederationClosed`           | The federation handle is closed, either because it was individually closed while retaining its data, or because the whole SDK instance was shut down.                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| <span id="code-timeout"></span>`Timeout`                             | The operation did not complete within an internal time budget.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| <span id="code-entropy"></span>`Entropy`                             | The platform's secure random source was unavailable or failed, so no entropy could be drawn. This is the one failure a caller can do nothing about: there is no input to correct, no permission to grant, and no retry that reliably helps. Report it and stop, and do not substitute entropy of your own.                                                                                                                                                                                                                                                                                                                      |
| <span id="code-storage"></span>`Storage`                             | The local storage backend failed to read or write. This is a fault in the backend itself (the I/O failed, or the device is out of space), so it is potentially transient, and retrying once the underlying condition is gone is reasonable. It does not cover a store whose contents are wrong: that is `StorageOrphaned`, `SeedMismatch` or `StorageInUse`.                                                                                                                                                                                                                                                                    |
| <span id="code-internal"></span>`Internal`                           | An internal error that does not fit any other category. Its presence generally indicates a bug.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
