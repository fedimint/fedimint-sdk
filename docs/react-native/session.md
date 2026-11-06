---
title: 'Share the session'
description: 'Open the SDK once in a module your app owns, share the session, and follow it with hooks.'
---

# Share the session

Open the SDK once in a module your app owns, share the session, and follow it from any screen with hooks.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="two hooks ship: useBalance and useOperation" /> <Badge type="tip" text="React only for the hooks" /> <Badge type="danger" text="no provider ships, on purpose" /></p>

[Seed words](./seed-words.md) said to open the SDK once and hand the same session to every screen. `@fedimint/react-native` does not ship a provider for this, on purpose. Your app decides when the wallet opens and when it closes. A provider that opens on mount and closes on unmount would take that choice away, and a remount could close the wallet under a screen that still uses it.

| Piece                                                            | Who writes it | Does                                                                                              |
| ---------------------------------------------------------------- | ------------- | ------------------------------------------------------------------------------------------------- |
| `src/fedimint.ts`                                                | You           | Opens the SDK once, hands every caller the same `SdkSession`, and closes it when your app says so |
| `SessionProvider`, [`useSession()`](./api-session-usesession.md) | You, optional | A small React context that holds the session                                                      |
| `useBalance(fed, onError?)`                                      | The SDK       | A federation's balance in msat, kept current. `undefined` until the first value                   |
| `useOperation(op, onError?)`                                     | The SDK       | An operation's latest state, kept current. `undefined` until the first value                      |

### 01 · Open once, in a module of your own <Badge type="info" text="JavaScript" /> {#step-01}

Put the session in one module. `open()` calls `openSdk` the first time and gives every later caller the same session, even a caller that comes while the first open is still running. [`close()`](./api-session-close.md) is the only place the session closes.

```ts [src/fedimint.ts]
import RNFS from 'react-native-fs'
import { openSdk, type SdkSession } from '@fedimint/react-native'

// A plain path, not a file:// URI. RNFS gives a plain path.
const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint`

let session: SdkSession | undefined
let opening: Promise<SdkSession> | undefined
let closing: Promise<void> | undefined

/** Opens the SDK once. Every caller gets the same session. */
export function open(): Promise<SdkSession> {
  opening ??= (async () => {
    // A close that is still running holds the folder. Wait for it, or the open fails with StorageInUse.
    await closing?.catch(() => undefined)
    return openSdk({ dataDir })
  })().then(
    (opened) => {
      session = opened
      return opened
    },
    (err) => {
      opening = undefined // let the next call try again
      throw err
    },
  )
  return opening
}

/** Closes the session. Every federation and operation from it stops working. */
export function close(): Promise<void> {
  const pending = opening
  session = undefined
  opening = undefined
  if (pending) closing = pending.then((s) => s.close())
  return closing ?? Promise.resolve()
}

/** The open session, or undefined before open() resolves and after close(). */
export const current = (): SdkSession | undefined => session
```

Gate the tree on it once, near the top. The root opens the session and does not close it on unmount. An unmount is not the end of the wallet's life. Your app calls `close()` at the points it chooses, such as sign out or before a restore over the same folder.

```tsx [src/WalletRoot.tsx]
import React, { useEffect, useState } from 'react'
import { Text } from 'react-native'
import type { SdkSession } from '@fedimint/react-native'
import { open } from './fedimint'

/** Put this near the top of your tree, once. */
export function WalletRoot({ children }: { children: React.ReactNode }) {
  const [session, setSession] = useState<SdkSession | undefined>(undefined)
  const [failed, setFailed] = useState<string | undefined>(undefined)

  useEffect(() => {
    let live = true
    open().then(
      (opened) => {
        if (live) setSession(opened)
      },
      (err) => {
        if (live) setFailed(String(err))
      },
    )
    // No close() here. The app closes the session, not a component.
    return () => {
      live = false
    }
  }, [])

  if (failed) return <Text>Could not open the wallet: {failed}</Text>
  if (!session) return <Text>Opening the wallet...</Text>
  return <>{children}</>
}
```

Below `WalletRoot`, any code can call `current()` and get the open session. Code outside React, such as a background task, calls `await open()` and gets the same one.

::: warning
**One module, one session.** Two calls to `openSdk` on the same folder do not share anything. The second fails with `StorageInUse`. Call `open()` from this module everywhere, and never call `openSdk` anywhere else.
:::

::: tip Check
Wrap your navigator in `WalletRoot`. You will see `Opening the wallet...` for a moment, then your first screen.
:::

### 02 · Optional: hold the session in a context <Badge type="info" text="JavaScript" /> {#step-02}

`current()` is enough for many apps. If you prefer to pass the session through React, write a small context. This is your code, not part of the SDK. It holds a session that is already open. It does not open one, and it does not close one.

```tsx [src/SessionContext.tsx]
import React, { createContext, useContext } from 'react'
import type { SdkSession } from '@fedimint/react-native'

const SessionContext = createContext<SdkSession | undefined>(undefined)

/** Your own provider. It holds an open session and never closes it. */
export function SessionProvider({
  session,
  children,
}: {
  session: SdkSession
  children: React.ReactNode
}) {
  return (
    <SessionContext.Provider value={session}>
      {children}
    </SessionContext.Provider>
  )
}

/** The session from the nearest SessionProvider. Throws when there is none above. */
export function useSession(): SdkSession {
  const session = useContext(SessionContext)
  if (!session) throw new Error('useSession needs a SessionProvider above it')
  return session
}
```

In `WalletRoot` from step 01, change the last line to `return <SessionProvider session={session}>{children}</SessionProvider>;` and import `SessionProvider` from `./SessionContext`. Then any screen below can call `useSession().sdk`.

::: tip Check
A screen below `WalletRoot` that renders `useSession().sdk.federations().length` shows `0` before the first join and `1` after it.
:::

### 03 · Follow a balance and an operation with hooks <Badge type="info" text="JavaScript" /> {#step-03}

The SDK ships two hooks. `useBalance(fed, onError)` keeps a balance on screen. `useOperation(op, onError)` keeps an operation's state on screen: a receive, a send, or one reopened from history. Both accept `undefined`, so a screen can call them before its federation or operation is ready. That keeps the hooks in the same order on every render, which React requires.

```tsx [InvoiceStatus.tsx]
import React from 'react'
import { Text } from 'react-native'
import {
  LnReceiveState_Tags,
  useOperation,
  type LnReceiveOperationLike,
} from '@fedimint/react-native'

/** Renders again on every state change, and stops listening when it unmounts. */
export function InvoiceStatus({
  operation,
}: {
  operation: LnReceiveOperationLike | undefined
}) {
  const state = useOperation(operation, (err) =>
    console.log('operation watch', err),
  )

  if (!state) return <Text>Creating the invoice...</Text>
  if (state.tag === LnReceiveState_Tags.Claimed) return <Text>Received</Text>
  return <Text>{state.tag}</Text>
}
```

Each hook is keyed on the object's `id()`. A new handle with the same id does not restart the watch. The hooks also never retry. When the federation or the session closes, the watch fails with `FederationClosed`, reports it to `onError`, and stops. After you reopen, remount the component with a new React `key`:

```tsx [BalanceCard.tsx]
import React, { useState } from 'react'
import { Button, Text } from 'react-native'
import {
  ErrorCode,
  Exception,
  useBalance,
  type FederationLike,
} from '@fedimint/react-native'

function BalanceLine({
  fed,
  onClosed,
}: {
  fed: FederationLike
  onClosed: () => void
}) {
  const msat = useBalance(fed, (err) => {
    if (
      Exception.hasInner(err) &&
      Exception.getInner(err).code() === ErrorCode.FederationClosed
    ) {
      onClosed()
    } else {
      console.log('balance watch', err)
    }
  })
  return (
    <Text>{msat === undefined ? 'loading...' : `${msat / 1000n} sats`}</Text>
  )
}

type Props = {
  fed: FederationLike
  /** Your code: open the session again and get the federation back, as in Join a federation. */
  reopen: () => Promise<FederationLike>
}

export function BalanceCard({ fed, reopen }: Props) {
  const [current, setCurrent] = useState(fed)
  const [watch, setWatch] = useState(0)
  const [closed, setClosed] = useState(false)

  if (closed) {
    return (
      <Button
        title="Reconnect"
        onPress={async () => {
          setCurrent(await reopen())
          setClosed(false)
          // Same id, so only a new key starts a new watch.
          setWatch((n) => n + 1)
        }}
      />
    )
  }
  return (
    <BalanceLine key={watch} fed={current} onClosed={() => setClosed(true)} />
  )
}
```

::: tip Check
Create an invoice as in [Receive money](./receive.md) and render `InvoiceStatus` with its operation. You will see `WaitingForPayment`, then `Received` once the invoice is paid, with no code of yours in between.
:::

## What you just did {#what-you-just-did}

You opened the SDK once, in a module your app owns, and shared the session. You held it in an optional context of your own, and followed a balance and an operation with the SDK's hooks. The app owns the session's life: it opens it, and it closes it.

[Restore from seed words](./restore.md) covers the one time the SDK is opened differently: when the user brings their seed words to a new phone.
