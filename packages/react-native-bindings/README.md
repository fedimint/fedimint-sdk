# @fedimint/react-native-bindings

Low-level React Native bindings for Fedimint client SDK.

## Installation

```bash
npm install @fedimint/react-native-bindings
# or
yarn add @fedimint/react-native-bindings
# or
pnpm add @fedimint/react-native-bindings
```

Binary artifacts for Android and iOS are automatically downloaded during installation.

### Skipping Binary Downloads

To skip automatic binary download (useful for local development or custom builds):

```bash
FEDIMINT_SKIP_BINARY_DOWNLOAD=true npm install @fedimint/react-native-bindings
```

## Usage

For most use cases, we recommend using the higher-level `@fedimint/react-native` package which provides a simpler API built on top of these bindings.

```typescript
// Direct low-level usage
import { rpcHandler } from '@fedimint/react-native-bindings';
```

## For Higher-Level API

Use the `@fedimint/react-native` package which wraps these bindings with a more convenient API:

```bash
npm install @fedimint/react-native
```

## Supported Platforms

The `@fedimint/react-native-bindings` package contains pre-compiled native Rust binaries. It explicitly supports the following architectures, which cover the vast majority of modern devices and simulators needed for React Native development:

### Android
*(Minimum Supported Version: Android 7.0 / API Level 24)*
* **`arm64-v8a`**: Required for all modern 64-bit physical Android devices (e.g., Samsung Galaxy S-series, Google Pixel).
* **`x86_64`**: Required for running the app on Android Emulators running on modern Mac/PC laptops.



### iOS
*(Minimum Supported Version: iOS 15.0)*
* **`aarch64-apple-ios`**: Required for all physical iOS devices (iPhone, iPad).
* **`x86_64-apple-ios`**: Required for running the app on the iOS Simulator on Intel-based Macs.

## Credit

Used the [bdk-rn](https://github.com/bitcoindevkit/bdk-rn) and [spark-sdk](https://github.com/breez/spark-sdk/tree/main) libraries as a reference.
