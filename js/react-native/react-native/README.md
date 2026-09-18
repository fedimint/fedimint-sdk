# @fedimint/react-native

The Fedimint SDK for React Native: `rust/fedimint-sdk` through its JSI turbo-module bindings
(`@fedimint/react-native-bindings`), the same API `@fedimint/sdk-web` exposes over a worker, but
called directly since there is no worker boundary to cross here.

## Implementation Options

Depending on your project setup, you can use the SDK with or without Expo. Choose the option
below that fits your environment.

### Option 1: Without Expo (Bare React Native)

#### Installation

```bash
npm install @fedimint/react-native
# or
yarn add @fedimint/react-native
# or
pnpm add @fedimint/react-native
```

You'll also need `react-native-fs` for a documents directory to store data in:

```bash
npm install react-native-fs
```

#### Usage

```typescript
import { openSdk, Exception } from '@fedimint/react-native'
import RNFS from 'react-native-fs'

const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint`
const { sdk, InviteCode, close } = await openSdk({ dataDir })

try {
  const federation = await sdk.join(InviteCode.parse(inviteCode))
  // ... use `federation` and the rest of the generated API.
} catch (error) {
  if (error instanceof Exception) {
    console.error(error.code(), error.reason(), error.details())
  }
  throw error
} finally {
  await close()
}
```

### Option 2: With Expo

#### Installation

```bash
npx expo install @fedimint/react-native expo-file-system
```

For Expo managed workflow (SDK 52+), add the plugin to your `app.json`:

```json
{
  "expo": {
    "plugins": ["@fedimint/react-native"]
  }
}
```

Then build with EAS or a custom dev client:

```bash
npx expo prebuild
npx expo run:ios
# or
npx expo run:android
```

**Note:** Expo Go is not supported. You must use a custom dev client.

#### Usage

```typescript
import { openSdk } from '@fedimint/react-native'
import { Paths } from 'expo-file-system'

// Strip the file:// scheme from the documents directory URI to get a plain
// filesystem path for Rust.
const dataDir = Paths.document.uri.replace(/^file:\/\//, '') + 'fedimint'

const { sdk, InviteCode, close } = await openSdk({ dataDir })
const federation = await sdk.join(InviteCode.parse(inviteCode))
```

#### Plugin Options

This is required for Expo managed workflow.

```json
{
  "expo": {
    "plugins": [
      [
        "@fedimint/react-native",
        {
          "skipBinaryDownload": false
        }
      ]
    ]
  }
}
```

### Building from Source

You can choose to build the SDK from scratch (recompile from source) and skip the automatic
binary download during installation by:

1. **Using an environment variable:**

   ```bash
   FEDIMINT_SKIP_BINARY_DOWNLOAD=true npm install @fedimint/react-native
   ```

2. **Using the Expo plugin option** (for Expo projects):
   Set `"skipBinaryDownload": true` in the plugin options above.

This is useful when you want to handle binary downloads manually or are building from source.

## Requirements

| React Native    | Support        |
| --------------- | -------------- |
| 0.78.x - 0.82.x | ✅ Supported   |
| 0.83.x          | ✅ Recommended |

| Platform | Minimum Version      |
| -------- | -------------------- |
| Android  | API 24 (Android 7.0) |
| iOS      | 15.0                 |

| Expo SDK | Support          |
| -------- | ---------------- |
| 52+      | ✅ With plugins  |
| Expo Go  | ❌ Not supported |

## API

`openSdk({ dataDir, mnemonic? })` returns `{ sdk, InviteCode, Notes, Mnemonic, close }`. `sdk`
and everything reachable from it (federations, operations, quotes, and so on) is the generated
API of `@fedimint/react-native-bindings`: every class, enum and method it exports, called
directly since JSI calls need no round trip. This package re-exports that whole module, so any
type or function documented there (`InviteCode`, `Notes`, `Mnemonic`, tagged enums such as
`LnReceiveState`, and so on) is available from `@fedimint/react-native` directly.

Failures surface as `Exception`, with `code()` giving the stable error code to branch on,
`reason()` a human-readable message (never parsed, only logged), and `details()` structured
detail where the failure has any.

`close()` shuts the SDK down; every object obtained from the session is dead afterward. A second
call does nothing.

## License

MIT
