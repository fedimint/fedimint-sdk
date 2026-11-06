---
title: 'Seed words'
description: 'Open the wallet once, and show the seed words only to their owner.'
---

# Seed words

Open the wallet once, and show the seed words only to their owner.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="no federation needed" /> <Badge type="tip" text="synchronous export" /> <Badge type="tip" text="12 words on a new wallet" /></p>

The [installation guide](./installation.md) opened the wallet and counted its seed words. This page opens that call up. You will not add a feature here. You will learn two habits every later guide relies on: open the wallet once, and treat the seed words as the money. The third habit, reading a failure by its code, is on [the errors page](./errors.md).

Everything hangs off one object. `openSdk` gives you a session. Its `sdk` is the wallet: one seed, and any number of federations joined with it. Each federation you join is a `FederationLike`, and money moves through its `ecash()` and `lightning()` methods. Amounts are always `bigint` millisatoshis.

```text no-check
openSdk({ dataDir })                    one session, one seed
  sdk.exportMnemonic().words()          the words, for a backup screen
  sdk.preview(invite)                   what a federation is, before joining (Join a federation)
  sdk.join(invite)                      a federation (Join a federation)
  sdk.recover(invite)                   a federation, restored from a seed (Restore from seed words)
    fed.balance()                       bigint msat (Show a balance)
    fed.ecash()?.receive(notes)         an operation (Receive money)
    fed.lightning()?.quote(invoice)     then send(quote) (Send money)
    fed.activity(undefined, 50)         history (See the history)
  session.close()                       when the app is done
```

### 01 · Open the wallet once <Badge type="info" text="JavaScript" /> {#step-01}

A wallet folder can be open once at a time. The SDK holds the folder, and a second `openSdk` on it, from the same app or another one, fails at once with `StorageInUse`. So open it in one place, keep the promise, and hand the same session to every screen.

`dataDir` is a plain path, not a `file://` URI. With react-native-fs it is `` `${RNFS.DocumentDirectoryPath}/fedimint` ``. If your file library returns a `file://` URI, strip that prefix first.

```ts [open-wallet.ts]
import { openSdk, type SdkSession } from '@fedimint/react-native'

let opening: Promise<SdkSession> | undefined

/**
 * The one wallet for the whole app. Every caller gets the same promise, so the
 * folder is opened once. A second open of the same folder fails with StorageInUse.
 * dataDir is a plain path, such as `${RNFS.DocumentDirectoryPath}/fedimint`.
 */
export function openWallet(dataDir: string): Promise<SdkSession> {
  opening ??= openSdk({ dataDir }).catch((err: unknown) => {
    opening = undefined // let the next caller try again
    throw err
  })
  return opening
}

/** When the app is finished with the wallet. Every object it handed out stops working. */
export async function closeWallet(): Promise<void> {
  const session = await opening
  opening = undefined
  await session?.close()
}
```

::: info
In a React app [Share the session](./session.md) shows how to share the session with every screen. The rule stays the same: your app opens the wallet, and your app closes it.
:::

::: warning
**A JavaScript reload can leave the old wallet open.** When Metro reloads your code, the session from the old JavaScript can still hold the folder. If the new code then gets `StorageInUse`, stop the app and start it again. A user never sees this: a release build does not reload.
:::

::: warning
**To open a folder again, let go of everything from the first session.** [`close()`](./api-session-close.md) shuts the SDK down and destroys `sdk`. The folder comes free only when nothing else from that session is alive either: no federation, no operation, no subscription. An open before that fails with `StorageInUse`. It does not wait.
:::

::: tip Check
Call `openWallet(dataDir)` from two screens. You will see both get the same session, and no `StorageInUse`.
:::

### 02 · Show the seed words to their owner, and nobody else <Badge type="info" text="JavaScript" /> {#step-02}

[`sdk.exportMnemonic()`](./api-sdk-exportmnemonic.md) returns the seed at once, with no `await`, because the SDK holds it in memory. The result does not print itself. [`words()`](./api-values-mnemonic-words.md) is the separate step that gives you the words as strings. They are the whole wallet: anyone who reads them can restore it on another phone and spend everything in it.

```ts [backup-words.ts]
import type { SdkSession } from '@fedimint/react-native'

/**
 * Call this only from a backup screen that the user opened on purpose.
 *
 * Anyone who reads these words can take every sat in the wallet. Do not log them,
 * do not send them to analytics or a crash reporter, and do not store them yourself:
 * the wallet already keeps the seed in its own folder.
 */
export function wordsForBackupScreen(session: SdkSession): string[] {
  return session.sdk.exportMnemonic().words()
}
```

Read the words before you call `close()`. After it, `sdk` is destroyed and every call on it throws, `exportMnemonic` too.

::: warning
**Never log the words, and never put them in an error message.** A crash reporter, an analytics event or a debug overlay that includes them has copied the money to somebody else's server. Show them on a screen the user opened on purpose, and let the user write them down. A screenshot copies them too, often to a cloud photo backup. On Android, set `FLAG_SECURE` on the window while the words show. iOS cannot block a screenshot, so tell the user to write the words, not to capture the screen. [The restore guide](./restore.md) is where they come back.
:::

::: tip Check
Open the backup screen on a new wallet. You will see 12 words. Open it again and you will see the same 12 words in the same order.
:::

## What you just did {#what-you-just-did}

You opened the wallet once for the whole app and kept the seed words where only their owner will see them. [The errors page](./errors.md) shows how to turn the SDK's failures into messages a user can act on, and every later guide reuses all three habits.

[The join guide](./join.md) makes the first call that leaves the device.
