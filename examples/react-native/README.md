# Fedimint React Native Example App

This is a sample application demonstrating how to integrate and use the `@fedimint/react-native` SDK.

This app serves two primary purposes:
1. **Developer Sandbox:** A convenient environment for the SDK maintainers to test local changes to the native bindings and JavaScript APIs.
2. **Usage Example:** A reference implementation for developers on how to initialize a Fedimint client, connect to a federation, and perform basic operations using the React Native SDK.

## Prerequisites

Before running this example, ensure you have the standard React Native environment set up for your platform (Node.js, Watchman, Xcode for iOS, Android Studio for Android). 

You must also build the local monorepo packages first, as this example depends on the local workspace versions of the Fedimint SDK.

## Getting Started

Because this example depends on the local workspace versions of the Fedimint React Native SDK and its native Rust bindings, you must build them first. This process requires `nix` and `just` (refer to the [SDK contributing guide](../../docs/core/dev/react-native.md)).

From the root of the monorepo, build the native bindings for your target platform:

For Android:
```sh
just build-android
```

For iOS (macOS only):
```sh
just build-ios
```

These commands will automatically clone the required FFI repositories, configure the Nix environment, compile the Rust libraries, and build the TypeScript packages across the workspace.

Then, navigate to this example directory:

```sh
cd examples/react-native
```

### Running on iOS

First, install the CocoaPods dependencies. Since this app uses local paths to reference the React Native bindings, you must run pod install *after* the `just build-ios` step at the monorepo root.

```sh
cd ios
bundle install # only needed the first time
bundle exec pod install
cd ..
```

Start the application:

```sh
pnpm ios
```

### Running on Android

Start the application:

```sh
pnpm android
```

### Starting the Metro Bundler separately

If you prefer to start the Metro bundler manually:

```sh
pnpm start
```
