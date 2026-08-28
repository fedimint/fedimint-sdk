# @fedimint/react-native

React Native SDK for Fedimint - the easiest way to integrate Fedimint into your React Native app.

## Implementation Options

Depending on your project setup, you can use the SDK with or without Expo. Choose the option below that fits your environment.

### Option 1: Without Expo (Bare React Native)

#### Installation

```bash
npm install @fedimint/react-native
# or
yarn add @fedimint/react-native
# or
pnpm add @fedimint/react-native
```

You'll also need `react-native-fs` for database storage:

```bash
npm install react-native-fs
```

#### Usage

```typescript
import WalletDirector from '@fedimint/react-native'
import RNFS from 'react-native-fs'

// Create a wallet director with a database path
const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`
const director = new WalletDirector(dbPath)

// Generate a mnemonic
const words = await director.generateMnemonic()
console.log('Mnemonic:', words.join(' '))

// Create a wallet and join a federation
const wallet = await director.createWallet()
await wallet.joinFederation(inviteCode)

// Use wallet methods
const balance = await wallet.balance.getBalance()
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
import WalletDirector from '@fedimint/react-native'
import { Paths } from 'expo-file-system'

// Prepare Database Path
const dbUriPath = Paths.document.uri // e.g. file:///data/...
// Strip the file:// scheme to get the plain filesystem path for Rust
const dbPath = dbUriPath + 'fedimint_db'
const rustPath = dbPath.replace(/^file:\/\//, '')

// Create a wallet director with a database path
const director = new WalletDirector(rustPath)

// Generate a mnemonic
const words = await director.generateMnemonic()
console.log('Mnemonic:', words.join(' '))

// Create a wallet and join a federation
const wallet = await director.createWallet()
await wallet.joinFederation(inviteCode)

// Use wallet methods
const balance = await wallet.balance.getBalance()
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

You can choose to build the SDK from scratch (recompile from source) and skip the automatic binary download during installation by:

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

## Exports

```typescript
// Default export - simplified WalletDirector
import WalletDirector from '@fedimint/react-native'

// Named exports for advanced usage
import {
  WalletDirector, // Class with built-in transport
  ReactNativeTransport, // Transport layer (for custom setups)
  TransportClient, // Low-level client
} from '@fedimint/react-native'

// Types
import type { FedimintWallet } from '@fedimint/react-native'
```

## License

MIT
