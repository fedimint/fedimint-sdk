---
title: 'Restore from seed words'
description: 'Open a wallet from seed words and recover each federation.'
---

# Restore from seed words

Open a wallet from seed words and recover each federation.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="reports progress" /> <Badge type="tip" text="resumes after a restart" /> <Badge type="tip" text="refuses a different seed" /></p>

A user lost their phone, or bought a new one. They have the words from [Seed words](./seed-words.md) written down. This page turns those words back into a wallet, then asks each federation they belonged to for the money that belongs to that seed.

It takes two calls, and they do different jobs. `openSdk` with a `mnemonic` makes the words this wallet's seed. It talks to nobody. Then `sdk.recover(invite)`, once per federation, joins it and scans it for the seed's ecash. That scan can take minutes, so it comes back as an operation to watch.

```text no-check
words ─▶ openSdk({ dataDir, mnemonic: words })   the wallet, with the old seed, no money yet
             │
             ▼  once per federation
         sdk.recover(invite) ─▶ { federation, progress }
             │
             ▼
         progress: Running ─▶ Done      the money is back and spendable
                           └▶ Failed    stopped before it finished, still locked
                                          ▲
         sdk.resumeRecovery(id) ──────────┘  after a restart, or to retry
```

::: warning
**The words do not say which federations the wallet used.** Recovery needs an invite for each one. Your app has to ask the user, or keep that list somewhere they can get back to, such as the directory from [Join a federation](./join.md).
:::

### 01 · Take the words <Badge type="info" text="JavaScript" /> {#step-01}

Let the user type or paste the words with any spacing and any capitals. A quick count catches a missing word early. The real check, the checksum, happens when the wallet opens in step 02.

```ts [read-words.ts]
/**
 * Turns what the user typed into a list of words. The real check, the checksum,
 * happens when the wallet opens: a wrong word fails there with InvalidInput.
 */
export function readWords(typed: string): string[] | null {
  const words = typed.trim().toLowerCase().split(/\s+/)
  return words.length === 12 || words.length === 24 ? words : null
}
```

::: warning
**The same care as [Seed words](./seed-words.md) applies.** The text field holds the whole wallet. Turn off autocorrect and suggestions on it, never log it, and clear it when the screen closes.
:::

::: tip Check
Paste 12 words with extra spaces and a capital letter. You will see `readWords` return 12 lowercase words. Paste 11 and you will see `null`.
:::

### 02 · Open the wallet with those words <Badge type="info" text="JavaScript" /> {#step-02}

Pass the words as `mnemonic`. On a new phone the folder is empty, so the words become its seed. If the folder already holds the same seed, it opens as usual. If it holds a different one, the open refuses with `SeedMismatch` rather than mixing two wallets.

```ts [open-with-seed.ts]
import {
  ErrorCode,
  Exception,
  openSdk,
  type SdkSession,
} from '@fedimint/react-native'

/**
 * dataDir is the same plain path the app always uses. On a new phone it is empty,
 * so the words become this wallet's seed. Nothing is fetched: no federation is involved.
 */
export async function openRestored(
  dataDir: string,
  words: string[],
): Promise<SdkSession> {
  try {
    return await openSdk({ dataDir, mnemonic: words })
  } catch (err) {
    if (!Exception.hasInner(err)) throw err
    const code = Exception.getInner(err).code()
    if (code === ErrorCode.InvalidInput) {
      throw new Error(
        'Those words are not a valid seed. Check each one and the order.',
      )
    }
    if (code === ErrorCode.SeedMismatch) {
      throw new Error('This phone already holds a different wallet.')
    }
    throw err
  }
}
```

Every later launch opens the folder without `mnemonic`, exactly as in [Seed words](./seed-words.md).

::: warning
**Offer restore before the app opens the folder for the first time.** The first open of an empty folder creates a new seed, and after that the folder takes only that seed: other words fail with `SeedMismatch`. In the same run, an open also fails with `StorageInUse` while any federation, operation or subscription from the first session is still alive. A screen that unmounts does not free its objects at once, so [`close()`](./api-session-close.md) alone is not enough.
:::

::: tip Check
Open with the words, then call `session.sdk.exportMnemonic().words()`. You will see the same words, in the same order. Open with one word changed and you will see the `InvalidInput` message.
:::

### 03 · Recover each federation, and watch it <Badge type="info" text="JavaScript" /> {#step-03}

Use `recover`, not `join`, for a federation the old wallet belonged to. A plain join starts fresh and does not look for money the seed already has there. `recover` returns as soon as the scan has started, with the joined `federation` and a `progress` operation to watch.

```ts [recover-federation.ts]
import {
  InviteCode,
  RecoveryState_Tags,
  type RecoveryOperationLike,
  type SdkLike,
} from '@fedimint/react-native'

/** Watches one recovery until it ends. Returns true when the wallet is restored. */
export async function watchRecovery(
  progress: RecoveryOperationLike,
  show: (text: string) => void,
): Promise<boolean> {
  // The first value is the current state. undefined means the stream ended.
  for (const updates = progress.updates(); ; ) {
    const state = await updates.next()
    if (state === undefined) {
      return (await progress.awaitFinal()).tag === RecoveryState_Tags.Done
    }
    switch (state.tag) {
      case RecoveryState_Tags.Running: {
        const p = state.inner.progress // undefined until the scan knows its size
        show(
          p
            ? `Looking for your money: ${p.complete} of ${p.total}`
            : 'Looking for your money...',
        )
        break
      }
      case RecoveryState_Tags.Done:
        show('Your money is back.')
        return true
      case RecoveryState_Tags.Failed:
        // For you, not for the user: reason is free text.
        console.log('recovery stopped:', state.inner.reason)
        show('Recovery stopped before it finished.')
        return false
    }
  }
}

/** Call once per federation the user belonged to. Use recover, not join, for these. */
export async function recoverFederation(
  sdk: SdkLike,
  invite: string,
  show: (text: string) => void,
): Promise<boolean> {
  // The wallet keeps the federation in its folder. federationOnLaunch finds it there.
  const { progress } = await sdk.recover(InviteCode.parse(invite.trim()))
  return watchRecovery(progress, show)
}
```

| State     | Means                                                                                                                                                                                                                 | Money can move                                                       |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `Running` | Scanning the federation's history for this seed, then taking in what it found. `inner.progress` is `{ complete, total }`, or `undefined` until the scan knows its size. `complete` reaching `total` is not `Done` yet | No. Sends and receives fail with `Recovering`                        |
| `Done`    | Final. The wallet is restored in this federation, and the balance includes what the scan found                                                                                                                        | Yes                                                                  |
| `Failed`  | Final for this attempt. The scan stopped before it finished. `inner.reason` is for your logs                                                                                                                          | No. The federation stays locked until a later attempt reaches `Done` |

A screen can show the same thing with `useOperation(progress)` from [Share the session](./session.md).

::: warning
**Call `recover` once per federation.** A second call for the same federation fails with `AlreadyJoined`. To watch the scan again, or to retry one that ended in `Failed`, use `sdk.resumeRecovery(id)` in step 04.

A `recover` that throws `FederationUnreachable`, `Timeout` or `Storage` may already have joined. Do not treat it as "nothing happened". Read the id with `InviteCode.parse(invite).federationId()` and go to step 04.
:::

::: tip Check
Recover Fedi Testnet with the words of a wallet that held money there. You will see `Looking for your money` with a count that grows, then `Your money is back.` From `Done` on, the balance line shows the restored amount.
:::

### 04 · Pick a recovery up again after a restart <Badge type="info" text="JavaScript" /> {#step-04}

The scan keeps its place in the wallet folder. When the app opens the folder again, the SDK reopens each federation and carries on with its scan. If the last attempt ended in `Failed`, the reopen starts a new attempt by itself. What the app has lost is its handle to watch. `sdk.resumeRecovery(id)` gives it back, and needs no invite code.

```ts [resume-recoveries.ts]
import {
  RecoveryState_Tags,
  type RecoveryHandle,
  type SdkLike,
} from '@fedimint/react-native'

/**
 * At launch: a handle for each recovery that has not reached Done yet.
 * Pass each handle's progress to watchRecovery from step 03.
 */
export async function resumeRecoveries(
  sdk: SdkLike,
): Promise<RecoveryHandle[]> {
  const handles: RecoveryHandle[] = []
  for (const fed of sdk.federations()) {
    // undefined: this federation was joined, not recovered, so it was never locked.
    const state = await sdk.recoveryStatus(fed.id())
    if (state === undefined || state.tag === RecoveryState_Tags.Done) continue
    // Running: watches the attempt in progress. Failed: starts a new attempt.
    handles.push(await sdk.resumeRecovery(fed.id()))
  }
  return handles
}
```

`recoveryStatus` only reads. It starts nothing and contacts no guardian. Use it to hide the send and receive buttons while a federation is locked, rather than wait for a call to fail with `Recovering`.

::: tip Check
Start a recovery, stop the app during the scan, and start it again. You will see `resumeRecoveries` return one handle, and `watchRecovery` go on from `Running` to `Done`.
:::

## What you just did {#what-you-just-did}

You took the user's seed words, opened a wallet with them, asked each federation for the money that belongs to that seed, watched the scan until it finished, and picked it up again after a restart.

That is every guide. What this guide does not cover: on chain deposits and withdrawals through [`fed.onchain()`](./api-federation-onchain.md), and leaving a federation.
