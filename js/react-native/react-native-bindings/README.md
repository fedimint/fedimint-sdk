# @fedimint/react-native-bindings

The `#[uniffi::export]` API of `rust/fedimint-sdk`, generated for React Native by
`uniffi-bindgen-react-native`. It imports `@ubjs/core` at run time for the JSI/turbo-module
runtime the generated code plugs into.

Most apps should use `@fedimint/react-native` instead, which wraps this package's generated
classes and functions with a more convenient API.

## Regenerating

Run `just generate-sdk-rn-bindings` after any change to the crate's UniFFI surface, then commit
the result. CI regenerates on every PR and fails the build if the working tree differs
afterwards, so a stale commit is caught before merge.

The npm package `uniffi-bindgen-react-native` is a direct dependency of this package, but only
for what the turbo-module compiles against: its C++ runtime headers (`cpp/includes`, read by
CMake and Xcode) and its CocoaPod. Its own `ubrn` CLI generates for UniFFI 0.31, one minor
version behind the crate, so it is never run; `generate-sdk-rn-bindings.sh` refuses a `ubrn` that
resolves under `node_modules` and this package keeps no `ubrn:*` scripts.

## Native libraries

Android: `just generate-sdk-rn-bindings` reads `rust/fedimint-sdk` built for Android through nix
(`.#fedimint-sdk-android-jni`) and copies the resulting `.so` files into
`android/src/main/jniLibs/<abi>/`.

iOS: `just build-rn-ios`, run on macOS with Xcode installed, cross-compiles the crate for the
configured targets and links the result into `FedimintReactNativeBindingsFramework.xcframework`.

## Supported ABIs

- Android: `arm64-v8a`, `x86_64`.
- iOS: device (`aarch64-apple-ios`), arm64 simulator (`aarch64-apple-ios-sim`), x86_64 simulator.
