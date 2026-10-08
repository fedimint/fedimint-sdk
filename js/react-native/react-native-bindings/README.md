# @fedimint/react-native-bindings

The `#[uniffi::export]` API of `rust/fedimint-sdk`, generated for React Native by
`uniffi-bindgen-react-native`. It imports `@ubjs/core` at run time for the JSI/turbo-module
runtime the generated code plugs into.

Most apps should use `@fedimint/react-native` instead, which wraps this package's generated
classes and functions with a more convenient API. That package has this one as a peer
dependency: install both, at the same version, so that React Native links the native module.

## Native libraries

The package carries its native libraries: `libfedimint_sdk.so` per ABI under
`android/src/main/jniLibs/`, and `FedimintReactNativeBindingsFramework.xcframework` for iOS.
Nothing is downloaded or built when the package is installed.

## Supported ABIs

- Android: `arm64-v8a`, `x86_64`.
- iOS: device (`aarch64-apple-ios`), arm64 simulator (`aarch64-apple-ios-sim`), x86_64 simulator.
