---
title: 'Installation'
description: 'Install the two npm packages, link them on iOS and Android, and see the SDK answer on a device.'
---

# Installation

Add both packages, set up each platform, and see the SDK answer on a device.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="prebuilt native libraries" /> <Badge type="tip" text="iOS 15.0 and Android API 24 or newer" /> <Badge type="tip" text="Expo config plugin" /> <Badge type="danger" text="not available in Expo Go" /></p>

By the end of this page your wallet will show one line of text: a Rust program inside your app has stored a seed. That is a small result, and it is the right first result, because it proves every layer at once. If the line appears, both packages resolved, the native library linked, the bridge loaded and the Rust core answered.

::: warning
**These packages contain compiled Rust, so they cannot run inside Expo Go.** Expo Go ships a fixed set of native libraries and yours is not one of them.

Use a development build instead. Step 03 shows the Expo path: a config plugin, `npx expo prebuild`, and a development build. A bare React Native project skips step 03.

Everything else in Expo still works. The router, the asset pipeline, the config plugins and over the air updates are all unaffected. You give up one client app, not the framework.
:::

## Two packages, and why {#two-packages}

`@fedimint/react-native` is the package you import. It adds `openSdk`, which opens the SDK over a folder, and two React hooks, `useBalance` and `useOperation`. It also re-exports everything from the bindings, so every import in this guide comes from this one package.

`@fedimint/react-native-bindings` is the native part. It is generated from the Rust crate `rust/fedimint-sdk`, and it holds the prebuilt native libraries, the podspec and the Android build. Nothing compiles when you install it. Your app must list it too, because React Native autolinks only the packages your app names directly. A native library that only another package depends on is never linked.

| Item                              | Version           | Note                                                                |
| --------------------------------- | ----------------- | ------------------------------------------------------------------- |
| `@fedimint/react-native`          | `0.1.0-beta.2`    | Install the exact version                                           |
| `@fedimint/react-native-bindings` | `0.1.0-beta.2`    | The same version as the package above                               |
| React Native                      | `0.78` or newer   | `0.83` is recommended                                               |
| React                             | `18` or newer     |                                                                     |
| New Architecture                  | Required          | The bindings are a Turbo Module. There is no legacy bridge fallback |
| iOS                               | `15.0` or newer   |                                                                     |
| Android                           | API `24` or newer | ABIs `arm64-v8a` and `x86_64` only                                  |
| Expo                              | SDK `52` or newer | Development build only, not Expo Go                                 |

## The seven steps {#the-seven-steps}

Steps 01 to 05 are setup and each one can fail loudly. Steps 06 and 07 are the payoff. Work in order. Every step ends with something you can check, so you never carry a broken state into the next one.

### 01 · Check your toolchain <Badge type="info" text="Native" /> {#step-01}

Run these before you touch the project, because each failure below arrives later disguised as something else. You need no Rust toolchain: the native libraries come prebuilt.

```sh [check-toolchain.sh]
node --version        # 22.13 or newer
ruby --version        # must match your project's .ruby-version
xcodebuild -version   # 16.1 or newer, for iOS
pod --version         # 1.13 or newer, for iOS
java -version         # 17, for Android
echo "LANG=$LANG"     # must not be empty
```

::: warning
**If `LANG` is empty, fix it now:** `export LANG=en_US.UTF-8`, and add that line to your shell profile. CocoaPods will otherwise fail deep inside Ruby with `Unicode Normalization not appropriate for ASCII-8BIT`, which names Ruby internals and never mentions the locale.
:::

If `ruby --version` disagrees with the `.ruby-version` file in your project, install the pinned version with a version manager such as rbenv. If `pod` is missing, your project almost certainly has a `Gemfile`: run `bundle install`, then use `bundle exec pod` everywhere below.

Three Ruby problems come up often:

- If `rbenv install <version>` says `definition not found`, your ruby-build is too old. Run `brew upgrade ruby-build`, then install again.
- If `bundle install` fails, install the Bundler version that `Gemfile.lock` names under `BUNDLED WITH`: `gem install bundler:<version>`.
- In a non-interactive shell (CI, or an agent), rbenv is not active, so `ruby` is the macOS system Ruby. Run `eval "$(rbenv init - zsh)"` first, with `bash` in place of `zsh` in a bash shell. The name must match the shell, or the init fails with `complete: command not found`. Then check that `ruby -v` matches `.ruby-version`.

For Android, set two environment variables. Set `JAVA_HOME` to a JDK 17 or newer. The JBR that comes with Android Studio works: `/Applications/Android Studio.app/Contents/jbr/Contents/Home`. Set `ANDROID_HOME` to your Android SDK folder. On macOS it is usually `~/Library/Android/sdk`. Then open the SDK Manager and install the NDK and CMake. Install the NDK version that `ndkVersion` names in your app's Android build files.

::: tip Check
You will see five versions and a locale that is not empty.
:::

### 02 · Add both packages to your app <Badge type="info" text="JavaScript" /> {#step-02}

Install both packages at the same exact version. `--save-exact` writes the version into `package.json` with no range, so the two can never drift apart. The probe in step 06 also uses `react-native-fs` to find the documents folder. Skip that line if your app already has a file library.

```sh [install.sh]
npm install --save-exact @fedimint/react-native@0.1.0-beta.2 @fedimint/react-native-bindings@0.1.0-beta.2
npm install react-native-fs
ls node_modules/@fedimint/
```

The bindings package is a download of several hundred megabytes. A slow install is not a hang. Wait for it to finish.

::: tip Check
You will see both packages:

```sh no-check [packages]
$ ls node_modules/@fedimint/
react-native react-native-bindings
```

:::

### 03 · With Expo: add the plugin and prebuild <Badge type="info" text="Native" /> {#step-03}

Skip this step in a bare React Native project. With Expo SDK 52 or newer, add the config plugin to `app.json`. On Android it turns on AndroidX and the New Architecture. On iOS it does nothing.

```json [app.json]
{
  "expo": {
    "plugins": ["@fedimint/react-native"]
  }
}
```

If `app.json` already has a `plugins` list, add the entry to it. Then generate the native projects and make a development build. The development build replaces Expo Go on your device or simulator.

```sh [expo-prebuild.sh]
npx expo prebuild
npx expo run:ios
npx expo run:android
```

`npx expo prebuild` runs `pod install` for you, so an Expo project skips step 04. It still needs step 05. Set the ABIs again after `npx expo prebuild --clean`, because that command writes a new `android` folder.

::: tip Check
You will see new `ios` and `android` folders, and your app opens in its own development build, not in Expo Go.
:::

### 04 · Install the iOS pods <Badge type="info" text="Native" /> {#step-04}

Autolinking finds the bindings on its own, because your app lists them. You do not edit the `Podfile`. `pod install` runs in the `ios` folder, not in the project root. Set `LANG` first, as step 01 says.

```sh [pods.sh]
cd ios
export LANG=en_US.UTF-8
bundle exec pod install --repo-update
```

The `--repo-update` flag refreshes the CocoaPods spec index. Use it for the first install. Without it, an old cached index can fail the install.

Two pods arrive together. `ReactNativeBindings` holds the native library. `uniffi-bindgen-react-native` is the runtime that carries values across the boundary between JavaScript and Rust. CocoaPods does not take this second pod from `node_modules`. The `ReactNativeBindings` podspec asks for it by version, and CocoaPods downloads it from the CocoaPods trunk. `@fedimint/react-native` adds no pod, because it has no native code.

::: tip Check
Your `Podfile.lock` will name both:

```text no-check
ReactNativeBindings
uniffi-bindgen-react-native
```

:::

If they are missing, the package was not found. Ask the tooling what it can see before you change anything:

```sh [check-autolink.sh]
npx react-native config | grep -A3 react-native-bindings
```

### 05 · Set the Android ABIs <Badge type="info" text="Native" /> {#step-05}

The bindings ship native libraries for two Android ABIs only: `arm64-v8a` for phones and for emulators on Apple silicon Macs, and `x86_64` for emulators on Intel hosts. Tell Gradle to build only those two. Find the `reactNativeArchitectures` line in `android/gradle.properties` and replace it, or add it if it is missing.

```properties [android/gradle.properties]
reactNativeArchitectures=arm64-v8a,x86_64
```

There is no other Android step. Gradle picks the package up through autolinking on the next build.

::: tip Check
You will see the one line:

```sh no-check [abis]
$ grep reactNativeArchitectures android/gradle.properties
reactNativeArchitectures=arm64-v8a,x86_64
```

:::

### 06 · Call the SDK <Badge type="info" text="JavaScript" /> {#step-06}

This screen is a probe. It opens the SDK and asks how many seed words it holds. It joins nothing and talks to no network. Create the file whole.

Four things here will surprise you if you are coming from an ordinary library:

1. `openSdk` takes a `dataDir`, because the SDK owns a database on disk. Your app chooses where it lives. Give a plain path, not a `file://` URI. `RNFS.DocumentDirectoryPath` is already a plain path.
2. The first open of an empty folder creates a seed and stores it. Every later open reads the same seed back. You never generate one yourself.
3. The probe counts the words and never shows them. Seed words are shown only to their owner, on a screen they asked for. [Seed words](./seed-words.md) has that screen.
4. `session.close()` is required. It gives the folder back. Until then, a second open of the same folder fails with `StorageInUse`.

```tsx [screen/wallets/FedimintDebug.tsx]
import React, { useEffect, useState } from 'react'
import { SafeAreaView, Text, View } from 'react-native'
import RNFS from 'react-native-fs'
import {
  ErrorCode,
  Exception,
  openSdk,
  type SdkSession,
} from '@fedimint/react-native'

/** The error code and reason of a failure from the core, or the plain message. */
function explain(e: unknown): string {
  if (!Exception.hasInner(e)) return String(e)
  const inner = Exception.getInner(e)
  return `${ErrorCode[inner.code()]}: ${inner.reason()}`
}

export default function FedimintDebug() {
  const [status, setStatus] = useState<string>('calling the rust core...')

  useEffect(() => {
    let cancelled = false
    let session: SdkSession | undefined

    // Your app owns this path. Use the real documents directory from the start:
    // a simulator shortcut such as /tmp would not survive on a device.
    const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint`

    openSdk({ dataDir })
      .then((opened) => {
        if (cancelled) return opened.close()
        session = opened
        // Count the words. Never put them on a debug screen, and never log them.
        const count = opened.sdk.exportMnemonic().words().length
        setStatus(`the rust core answered: a ${count} word seed is stored`)
      })
      .catch((e: unknown) => {
        if (!cancelled) setStatus('FAILED: ' + explain(e))
      })

    // Gives the folder back. The cancelled flag stops a late reply from
    // setting state on a screen that has already gone.
    return () => {
      cancelled = true
      void session?.close()
    }
  }, [])

  return (
    <SafeAreaView>
      <View style={{ padding: 24 }}>
        <Text style={{ fontSize: 12, letterSpacing: 2, marginBottom: 12 }}>
          FEDIMINT DEBUG
        </Text>
        <Text style={{ fontSize: 18, lineHeight: 26 }}>{status}</Text>
      </View>
    </SafeAreaView>
  )
}
```

Then register the screen in your navigator, the same way you register any other. The SDK has no opinion about navigation.

::: tip Check
TypeScript will accept the file with no errors:

```sh no-check [typecheck]
$ npx tsc --noEmit
no output, exit 0
```

:::

### 07 · See the core answer <Badge type="info" text="Rust core" /> {#step-07}

Build and run. This is a full native build, because a new native library went in, so expect minutes rather than seconds. A JavaScript reload is not enough. With Expo, use `npx expo run:ios` or `npx expo run:android` instead.

```sh [run.sh]
npx react-native run-ios
npx react-native run-android
```

::: tip Check
Open the debug screen. It will say that a 12 word seed is stored. Open it a second time and it says the same: the seed was written once, on the first open, and read on the second.
:::

### If the screen says FAILED, or the build stops {#troubleshooting}

Read the message before you change anything. A failure from the core reaches JavaScript as an error that is not an `instanceof Exception`. The probe reads it with `Exception.hasInner(e)`, then `Exception.getInner(e).code()` and `.reason()`. The code is a number. `ErrorCode[code]` gives its name, such as `StorageInUse`. [Errors](./errors.md) lists every code. These cover almost every case.

```text no-check
TurboModuleRegistry.getEnforcing(...): 'ReactNativeBindings' could not be found
    The native side did not load. Your app does not list the bindings (step 02),
    pod install did not run after that (step 04), the New Architecture is off,
    or you ran an old build. Rebuild. A JavaScript reload cannot fix this.

Invalid hook call
    Two copies of React. Run npm ls react. Every entry must be your app's
    copy, marked deduped. If another version shows, pin react to your app's
    version with an overrides entry in package.json, then install again.

None of your spec sources contain a spec satisfying the dependency: uniffi-bindgen-react-native
    CocoaPods has an old cached index and does not know the pod version. Run
    bundle exec pod install --repo-update in the ios folder (step 04).

FAILED: StorageInUse: ...
    The folder is already open. Something opened the SDK twice, often a fast
    refresh while the first session was still open. Open it once per app
    (Share the session), and close the session before you open it again.
```

## What you just did {#what-you-just-did}

You added both packages at one exact version, set up iOS and Android, and called across three languages to get an answer back. Nothing on this page talked to a network. The seed was written once, on the first open, into the folder you named.

You also met the rules that shape every later page. The SDK owns a folder, so it needs a path. It remembers, so a seed is written once and read afterwards. It holds the folder until you close the session. And the seed words are the money, so they are never logged and never shown to anybody but their owner.

[Share the session](./session.md) goes back to `openSdk` and shows how to open once for the whole app. [Seed words](./seed-words.md) shows the seed words safely.
