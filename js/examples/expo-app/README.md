# Fedimint Expo Example App

This is a sample application demonstrating how to integrate and use the `@fedimint/react-native` SDK with [Expo Router](https://docs.expo.dev/router/introduction/) file-based navigation.

This app serves two primary purposes:
1. **Developer Sandbox:** A convenient environment for the SDK maintainers to test local changes to the native bindings and JavaScript APIs.
2. **Usage Example:** A reference implementation for developers on how to initialize a Fedimint client, connect to a federation, and perform basic operations using the React Native SDK.

It opens a wallet backed by a seed (generated on first launch or restored from
existing words), joins a federation from an invite code, shows the ecash
balance live, and walks through ecash, lightning and on-chain sends and
receives against that federation.

## Prerequisites

Before running this example, ensure you have the standard React Native environment set up for your platform (Node.js, Watchman, Xcode for iOS, Android Studio for Android).

You must also build the local monorepo packages first, as this example depends on the local workspace versions of the Fedimint SDK.

## Getting Started

From the repository root, ensure all dependencies are installed and the native bindings are built.
The pnpm workspace lives in `js/`, hence `--dir js`:

```sh
pnpm --dir js install
just build-rn-android      # nix-built Android libraries, regenerated bindings, JS build
just build-rn-ios          # macOS with Xcode only
```

Then, navigate to this example directory:

```sh
cd js/examples/expo-app
```

### Running on iOS

First, install the CocoaPods dependencies. Since this app uses local paths to reference the React Native bindings, you must run pod install *after* the `just build-rn-ios` step above.

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

With a device connected over USB debugging, start the application:

```sh
pnpm android
```

Without a device, the repository provides an emulator. From the repository root, boot it in one
terminal (the virtual device is created on first use; KVM access is required) and install the app
from another. Both commands run in the `.#android-emulator` shell, so the same `adb` owns the
device:

```sh
just android-emulator            # add -no-window for a headless boot
just rn-example expo-app
```

### Starting the Metro Bundler separately

If you prefer to start the Metro bundler manually:

```sh
pnpm start
```
