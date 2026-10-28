---
title: 'Quickstart'
description: 'From an empty app to a balance on screen in one page.'
---

# Quickstart

From an empty React Native app to a federation balance on screen.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="iOS 15.0 and Android API 24 or newer" /> <Badge type="tip" text="prebuilt native libraries" /> <Badge type="tip" text="New Architecture required" /> <Badge type="danger" text="not available in Expo Go" /></p>

::: info
This page is the short path. It joins Fedi Testnet, a test federation on signet, and shows its balance. Each step links to the page with the full detail and the reasons.
:::

### 01 · Install the two packages <Badge type="info" text="JavaScript" /> {#step-01}

Add both packages at the same exact version. Your app must list both, because autolinking links only the packages your app names. The packages ship prebuilt native libraries, so nothing compiles at install. The screen below also uses `react-native-fs` to find the documents folder.

```sh [install.sh]
npm install --save-exact @fedimint/react-native@0.1.0-beta.2 @fedimint/react-native-bindings@0.1.0-beta.2
npm install react-native-fs
ls node_modules/@fedimint/
```

Install the iOS pods. For Android, build only the two ABIs the packages ship. Set them in `android/gradle.properties`.

```sh [pods.sh]
cd ios
export LANG=en_US.UTF-8
bundle exec pod install
```

```properties [android/gradle.properties]
reactNativeArchitectures=arm64-v8a,x86_64
```

With Expo, add the config plugin and make a development build instead. [Installation step 03](./installation.md#step-03) shows how. [Installation](./installation.md) has every step, and what to do when one fails.

::: tip Check
You will see `react-native` and `react-native-bindings` in `node_modules/@fedimint/`, and `Pod installation complete!`.
:::

### 02 · Write the screen <Badge type="info" text="JavaScript" /> {#step-02}

Replace `App.tsx` with this file. It does four things:

1. `openWallet` opens the data folder once for the whole app. A second open of the same folder fails with `StorageInUse`. The first open of an empty folder creates the seed. [Share the session](./session.md) has the full detail.
2. `connectTestnet` parses the invite code. Then it reconnects to the federation if this device holds it, or joins it on the first launch. [Join a federation](./join.md) has the full detail.
3. `useBalance` returns the balance in msat, as a `bigint`. It is `undefined` until the first value arrives. Then it stays current. [Show a balance](./balance.md) has the full detail.
4. `explain` turns a failure into its error code and reason. [Errors](./errors.md) has the full detail.

```tsx [App.tsx]
import React, { useEffect, useState } from 'react'
import { SafeAreaView, Text } from 'react-native'
import RNFS from 'react-native-fs'
import {
  ErrorCode,
  Exception,
  InviteCode,
  openSdk,
  useBalance,
  type FederationLike,
  type SdkSession,
} from '@fedimint/react-native'

// Fedi Testnet, a test federation on signet. Its money has no value.
const TESTNET_INVITE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

let opening: Promise<SdkSession> | undefined

/** The one session for the whole app. Every caller gets the same promise. */
function openWallet(): Promise<SdkSession> {
  // A plain path, not a file:// URI.
  const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint`
  opening ??= openSdk({ dataDir }).catch((err: unknown) => {
    opening = undefined // let the next caller try again
    throw err
  })
  return opening
}

/** Reconnect to Fedi Testnet if this device holds it. Join it on the first launch. */
async function connectTestnet(session: SdkSession): Promise<FederationLike> {
  const { sdk } = session
  const invite = InviteCode.parse(TESTNET_INVITE.trim())
  const id = invite.federationId()
  const stored = sdk.storedFederations().some((f) => f.id === id)
  return (
    sdk.federation(id) ??
    (stored ? await sdk.reopenFederation(id) : await sdk.join(invite))
  )
}

function explain(e: unknown): string {
  if (!Exception.hasInner(e)) return String(e)
  const inner = Exception.getInner(e)
  return `${ErrorCode[inner.code()]}: ${inner.reason()}`
}

export default function App() {
  const [fed, setFed] = useState<FederationLike>()
  const [error, setError] = useState<string>()

  useEffect(() => {
    let cancelled = false
    openWallet()
      .then(connectTestnet)
      .then(
        (f) => {
          if (!cancelled) setFed(f)
        },
        (e: unknown) => {
          if (!cancelled) setError(explain(e))
        },
      )
    return () => {
      cancelled = true
    }
  }, [])

  // undefined until the first value arrives, then the balance in msat, kept current.
  const msat = useBalance(fed, (e) => setError(explain(e)))

  const line =
    error !== undefined
      ? `FAILED: ${error}`
      : msat === undefined
        ? 'loading...'
        : `${msat / 1000n} sats`

  return (
    <SafeAreaView style={{ padding: 24 }}>
      <Text style={{ fontSize: 14 }}>{fed?.name() ?? 'Fedi Testnet'}</Text>
      <Text style={{ fontSize: 32 }}>{line}</Text>
    </SafeAreaView>
  )
}
```

::: tip Check
You will see no errors from `npx tsc --noEmit`.
:::

### 03 · Run it <Badge type="info" text="Native" /> {#step-03}

Build and run. This is a full native build, because a new native library went in. A JavaScript reload is not enough.

```sh [run.sh]
npx react-native run-ios
npx react-native run-android
```

The first launch joins Fedi Testnet. Later launches reconnect to it. To see the number change, pay the wallet from a signet faucet. [Receive money](./receive.md) shows how to make an invoice.

::: tip Check
You will see the federation name and `0 sats`. If you see `FAILED:`, the line names the error code. [Installation](./installation.md#troubleshooting) lists the common ones.
:::

## Next {#next}

- [Receive money](./receive.md)
- [Send money](./send.md)
- [Share the session](./session.md)
