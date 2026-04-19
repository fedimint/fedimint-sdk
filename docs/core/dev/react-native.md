# React Native Development

This guide explains how to set up, develop, and build the React Native bindings and SDK for Fedimint.

The React Native packages in this repository bridge the Rust-based `fedimint-client-uniffi` into a React Native environment using [UniFFI](https://mozilla.github.io/uniffi-rs/) and JSI.

## Prerequisites

### Supported Versions & Architectures

The React Native SDK currently maintains compatibility with:

- **React Native**: `>= 0.78.0`
- **React**: `>= 18.0.0`

**Supported Native Architectures:**

- **Android** _(Min SDK 24 / Android 7.0)_:
  - `arm64-v8a`: Physical Android devices
  - `x86_64`: Android Emulators
- **iOS** _(Deployment Target 15.0+)_:
  - `aarch64-apple-ios` (`arm64`): Physical iOS devices
  - `aarch64-apple-ios-sim` (`arm64` Simulator): Apple Silicon Simulators
  - `x86_64-apple-ios` (`x86_64` Simulator): Intel Simulators

### Environment Requirements

Because React Native requires both Node.js tooling and native compilation (Rust, Android NDK, iOS Xcode toolchain), we heavily rely on **Nix** to provide reproducible environments.

1. **Nix**: Ensure Nix is installed. Refer to the [Nix Setup](./nix_setup.md) guide.
2. **Just**: We use `just` as a command runner to orchestrate the Nix shells and build commands. You can install it natively or rely on the Nix environment.
3. **macOS**: _Required if you want to build the iOS bindings._

## Building the Bindings

The React Native build process requires the `fedimint-sdk-ffi` repository to be cloned alongside your `fedimint-sdk` directory. Our `just` commands will automatically clone this for you if it is missing.

To execute a build, run the `just` commands from the root of the repository. These commands will automatically enter the strictly-defined Nix shells (`#android` or `#ios`) ensuring you use the precise Android NDK and iOS toolchains required.

### Android

To build the Android bindings and compile the React Native TypeScript packages:

```bash
just build-android
```

To build the optimized **release** version of the Android bindings:

```bash
just release-android
```

### iOS

_(macOS only)_

To build the iOS bindings (`.xcframework`) and compile the React Native TypeScript packages:

```bash
just build-ios
```

To build the optimized **release** version of the iOS bindings:

```bash
just release-ios
```

## What Happens Under the Hood?

When you run `just build-android` (or the iOS equivalent), the following steps are orchestrated:

1. **Clone FFI**: Checks for the existence of `fedimint-sdk-ffi` in the root and clones it if not found.
2. **Install Dependencies**: Runs `pnpm i` inside the default Nix shell.
3. **Build UniFFI Bindings**: Enters the target-specific Nix shell (e.g., `nix develop .#android`) and runs `pnpm run ubrn:android`. This invokes Cargo to cross-compile the Rust code and generate the native libraries (`jniLibs` or `xcframework`).
4. **Build React Native SDK**: Enters the Nix shell again to run `pnpm run build:reactnative`, which transpiles the TypeScript bridge code.

## Testing Local Changes

To see your local modifications to the React Native bindings in action, you can use the example applications provided in the repository. These applications are pre-configured to consume the locally built SDK packages:

- **`examples/expo-app`**: An Expo-based React Native application.
- **`examples/react-native`**: A bare React Native application.

After running the `just build-*` commands, refer to the respective `README.md` files inside these example directories to start the development servers and run the apps on an emulator or physical device.
