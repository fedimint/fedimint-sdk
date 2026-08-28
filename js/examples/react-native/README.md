# Fedimint React Native Example App

This is a sample application demonstrating how to integrate and use the `@fedimint/react-native` SDK.

This app serves two primary purposes:
1. **Developer Sandbox:** A convenient environment for the SDK maintainers to test local changes to the native bindings and JavaScript APIs.
2. **Usage Example:** A reference implementation for developers on how to initialize a Fedimint client, connect to a federation, and perform basic operations using the React Native SDK.

## Prerequisites

Before running this example, ensure you have the standard React Native environment set up for your platform (Node.js, Watchman, Xcode for iOS, Android Studio for Android). 

You must also build the local monorepo packages first, as this example depends on the local workspace versions of the Fedimint SDK.

## Getting Started

From the repository root, ensure all dependencies are installed and the native bindings are built.
The pnpm workspace lives in `js/`, hence `--dir js`:

```sh
pnpm --dir js install
nix develop .#android -c pnpm --dir js ubrn:android
nix develop .#ios -c pnpm --dir js ubrn:ios
pnpm --dir js build:reactnative
pnpm --dir js build
```

Then, navigate to this example directory:

```sh
cd js/examples/react-native
```

### Running on iOS

First, install the CocoaPods dependencies. Since this app uses local paths to reference the React Native bindings, you must run pod install *after* the `pnpm --dir js build` step above.

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
