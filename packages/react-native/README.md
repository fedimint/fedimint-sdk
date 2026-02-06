# @fedimint/react-native

React Native SDK for Fedimint - the easiest way to integrate Fedimint into your React Native app.

## Installation

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

## Usage

```typescript
import WalletDirector from '@fedimint/react-native'
import RNFS from 'react-native-fs'

// Create a wallet director with a database path
const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`
const director = new WalletDirector(dbPath)

// Preview a federation before joining
const preview = await director.previewFederation(inviteCode)
console.log('Federation:', preview.config.global.meta.federation_name)

// Create a wallet and join a federation
const wallet = await director.createWallet()
await wallet.joinFederation(inviteCode)

// Use wallet methods
const balance = await wallet.balance.getBalance()
```

## Expo Support

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

### Plugin Options

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

### Skipping Binary Downloads

You can skip the automatic binary download during installation by:

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
| iOS      | 13.4                 |

| Expo SDK | Support                   |
| -------- | ------------------------- |
| 52+      | ✅ With custom dev client |
| Expo Go  | ❌ Not supported          |

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
