# Fedimint Expo Example App

This is a sample application demonstrating how to integrate and use the `@fedimint/react-native` SDK with [Expo Router](https://docs.expo.dev/router/introduction/) file-based navigation.

This app serves two primary purposes:
1. **Developer Sandbox:** A convenient environment for the SDK maintainers to test local changes to the native bindings and JavaScript APIs.
2. **Usage Example:** A reference implementation for developers on how to initialize a Fedimint client, connect to a federation, and perform basic operations using the React Native SDK.

## Prerequisites

Before running this example, ensure you have the standard React Native environment set up for your platform (Node.js, Watchman, Xcode for iOS, Android Studio for Android).

You must also build the local monorepo packages first, as this example depends on the local workspace versions of the Fedimint SDK.

## Getting Started

From the root of the monorepo, you must first initialize the Rust FFI submodules. This pulls down the core Fedimint Rust code needed to compile the bindings:

```sh
git submodule update --init --recursive
```

Then, ensure all dependencies are installed and the native bindings are built:

```sh
pnpm install
nix develop .#android -c pnpm ubrn:android
nix develop .#ios -c pnpm ubrn:ios
pnpm build:reactnative
pnpm build
```

Then, navigate to this example directory:

```sh
cd examples/expo-app
```

### Running on iOS

First, install the CocoaPods dependencies. Since this app uses local paths to reference the React Native bindings, you must run pod install *after* the `pnpm build` step at the monorepo root.

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
