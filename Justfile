set shell := ["bash", "-c"]

# Native libraries only (.so), via Nix. Shared by every binding generator,
# and what the two-job CI split (android-native.yaml, android-sdk.yaml) uses
# so a binding job can read an artifact instead of rebuilding.
build-android-so:
    ./scripts/nix-build-android-so.sh

# The Kotlin bindings, read out of an already-built .so.
build-android-bindings:
    ./scripts/generate-android-bindings.sh

build-android-sdk:
    ./scripts/build-android-sdk.sh

# Compile the library and the demo app against the freshly generated bindings.
test-android-sdk: build-android-sdk
    cd android && ./gradlew :fedimint-sdk:assembleRelease :app:assembleDebug

# The example APK the E2E suite installs: the native library, the Kotlin generated
# from it, then Gradle, all in the lean `.#android` shell (Android SDK and a
# JDK — no emulator, no devimint). CI runs the same three steps as one job each
# (native, bindings, apk), each handed the last one's output. It is a separate
# step from `test-android-e2e` on purpose: a Gradle
# build alongside an emulator and a devimint federation starves the emulator
# until Android's System UI stops responding, so the build finishes — daemon
# and all — before either of those starts.
build-android-apk:
    nix develop --accept-flake-config .#android -c bash -c './scripts/build-android-sdk.sh && cd android && ./gradlew :app:assembleDebug'

# Boot an emulator, install the example app and drive it with the Appium suite
# (js/android/integration-tests) against a devimint federation — the Android
# counterpart of `just test`, which does the same for the browser package's
# tests (js/web/web-sdk), through the same scripts/setup_test_shell.sh. There is
# deliberately no federation-free variant: one way to run this suite, so what CI
# does and what you can reproduce are the same thing. Builds the APK first (see
# build-android-apk); the script itself only installs one. `shape` picks the
# federation's module generation: v1 (mint, wallet, ln) or v2 (mintv2, walletv2,
# lnv2).
test-android-e2e shape="v1": build-android-apk
    FM_SDK_SHAPE={{shape}} nix develop --accept-flake-config .#android-tests -c scripts/setup_test_shell.sh bash scripts/e2e-android/run-android-e2e.sh

# The cross-platform SDK suite (js/e2e) on one target against a devimint
# federation: builds the target's harness first, then runs scripts/e2e/run.sh
# inside the target's nix shell, which only installs it. `shape` is v1 (mint,
# wallet, ln) or v2 (mintv2, walletv2, lnv2).
test-e2e target="android" shape="v1":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{target}}" in
      android) shell=android-tests ;;
      ios) shell=ios-tests ;;
      web) shell=wasm-tests ;;
      *) echo "unknown e2e target '{{target}}' (known: android, ios, web)" >&2; exit 1 ;;
    esac
    just build-e2e-{{target}}
    FM_SDK_SHAPE={{shape}} nix develop --accept-flake-config ".#$shell" -c scripts/setup_test_shell.sh bash scripts/e2e/run.sh {{target}}

# The Android harness: the debug APK, whose harness.ConsoleActivity js/e2e drives.
build-e2e-android: build-android-apk

# The iOS harness: the XCFramework and Swift bindings, then the harness app js/e2e drives,
# built for the simulator into ios/Harness/build.
build-e2e-ios: build-swift
    nix develop --accept-flake-config .#ios -c bash -c 'cd ios/Harness && xcodegen generate'
    cd ios/Harness && xcodebuild build -project FedimintHarness.xcodeproj -scheme FedimintHarness -destination 'generic/platform=iOS Simulator' -derivedDataPath build CODE_SIGNING_ALLOWED=NO

# The iOS harness for a connected device, signed by `team` under `bundle_id` (a free team cannot
# sign org.fedimint.harness). Run it with IOS_DEVICE=<udid> IOS_TEAM_ID=<team> set for
# scripts/e2e/ios.sh; only the federation-free tests pass there, since a device cannot reach
# devimint on the host's loopback.
build-e2e-ios-device team bundle_id: build-swift
    nix develop --accept-flake-config .#ios -c bash -c 'cd ios/Harness && xcodegen generate'
    cd ios/Harness && xcodebuild build -project FedimintHarness.xcodeproj -scheme FedimintHarness -destination 'generic/platform=iOS' -derivedDataPath build -allowProvisioningUpdates -allowProvisioningDeviceRegistration CODE_SIGNING_ALLOWED=YES CODE_SIGNING_REQUIRED=YES CODE_SIGN_STYLE=Automatic DEVELOPMENT_TEAM={{team}} PRODUCT_BUNDLE_IDENTIFIER={{bundle_id}}

# The web harness: the browser package over freshly generated bindings, and the harness page
# js/e2e drives, bundled into js/web/sdk-harness/dist.
build-e2e-web: generate-sdk-web-bindings
    nix develop --accept-flake-config .#wasm -c bash -c 'pnpm --dir js build && pnpm --dir js/web/sdk-harness bundle'

# Test the release checks: the component, version and changelog scripts the
# release, tag and bump workflows rely on. Needs only bash and git, no build.
test-sdk-release-scripts:
    ./scripts/test-sdk-release-scripts.sh

# Test the React Native scripts: how the packages are packed and checked, when they are published,
# and how the iOS libraries are built, have their symbols hidden and are assembled into the
# xcframework. Needs only bash, git and node, no build.
test-react-native-sdk-scripts:
    ./scripts/test-react-native-sdk-scripts.sh
    ./scripts/test-assemble-rn-ios-xcframework.sh
    ./scripts/test-build-sdk-rn-ios.sh
    ./scripts/test-hide-ios-symbols.sh

# Test the script behind the wasm module size check of a pull request. Needs only bash, gzip and
# the usual POSIX tools, no build.
test-sdk-web-scripts:
    ./scripts/test-sdk-web-wasm-size.sh

# Assemble the release AAR. Publishing to Maven Central is .github/workflows/android-sdk-release.yaml.
build-android-aar: build-android-sdk
    cd android && ./gradlew :fedimint-sdk:assembleRelease

# Non-nix escape hatch: cross-compile + generate locally with cargo-ndk.
# Needs the `.#android` shell (NDK, cargo-ndk, cmake/go for aws-lc-sys).
build-android-local:
    nix develop --accept-flake-config .#android -c ./scripts/build-android-sdk.sh --local

# Boot the emulator the React Native example apps run on (created on first use, see the
# script). Leave it running; pass emulator flags after the recipe name, e.g. -no-window.
rn-android-emulator *ARGS:
    nix develop --accept-flake-config .#rn-android-emulator -c scripts/rn-android-emulator.sh {{ARGS}}

# Build and install one example app (react-native or expo-app) on the running emulator or a
# connected device and launch it, from the same shell as the emulator so one adb owns the device.
# Needs `just build-rn-android` first. See the script for why this is not `react-native
# run-android`.
rn-android-example app="react-native":
    nix develop --accept-flake-config .#rn-android-emulator -c scripts/rn-android-example.sh {{app}}

# The Apple native libraries only (.a), one per target. Shared by every Apple
# binding generator, the same way build-android-so is for Android.
#
# Not a Nix build: every Apple target compiles against an SDK that ships inside
# Xcode and cannot live in the store, so there is no cacheable derivation to
# make. `.#ios` supplies the Rust targets and the C build tools; Xcode supplies
# the SDKs. macOS host with Xcode required.
#
# IOS_TARGETS="aarch64-apple-darwin" narrows it to the host slice, which is all
# `test-swift` needs and is roughly a quarter of the work.
build-ios-lib:
    nix develop --accept-flake-config .#ios -c ./scripts/build-ios-lib.sh

# The Apple native libraries via Nix — cachix-cached, see nix/ffi.nix. Nothing
# compiles locally when the cache is warm; a cold run cross-compiles the crate
# (heavy: rocksdb + aws-lc from C). This is what `build-swift` uses by default.
build-ios-lib-nix:
    ./scripts/nix-build-ios-lib.sh

# The Swift bindings, read out of an already-built .a.
build-swift-bindings:
    ./scripts/generate-swift-bindings.sh

build-swift:
    ./scripts/build-ios-sdk.sh

# Non-nix escape hatch: cross-compile locally with plain cargo instead of
# fetching from Cachix, for a machine that cannot or should not use Nix.
# Needs the `.#ios` shell (Apple Rust targets, cmake/perl/go for aws-lc-sys).
build-swift-local:
    nix develop --accept-flake-config .#ios -c ./scripts/build-ios-sdk.sh --local

# Runs on the macOS slice of the XCFramework — no simulator boot needed.
test-swift: build-swift
    swift test --package-path ios

# Compile the demo app against the freshly generated bindings, the Swift
# counterpart of `test-android-sdk`'s :app:assembleDebug. The .xcodeproj is
# generated from the committed project.yml and gitignored.
# No ARCHS override: this depends on `build-swift`, whose default target set
# includes both simulator architectures, so the lipo'd slice satisfies the
# generic destination. A subset build (IOS_TARGETS) needs ARCHS narrowed to
# match — see .github/workflows/swift-sdk.yaml.
build-ios-demo: build-swift
    nix develop --accept-flake-config .#ios -c bash -c 'cd ios/Demo && xcodegen generate'
    cd ios/Demo && xcodebuild build -project FedimintDemo.xcodeproj -scheme FedimintDemo -destination 'generic/platform=iOS Simulator'

test:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test

# The browser package's bindings, regenerated from the Nix-built wasm module. Run after any
# change to rust/fedimint-sdk's UniFFI surface and commit the result; CI checks it is fresh.
generate-sdk-web-bindings:
    nix develop --accept-flake-config .#wasm -c pnpm --dir js install
    nix develop --accept-flake-config .#wasm -c scripts/generate-sdk-web-bindings.sh

# The React Native package's bindings, regenerated from the nix-built Android libraries. Run after
# any change to rust/fedimint-sdk's UniFFI surface and commit the result; CI checks it is fresh.
generate-sdk-rn-bindings:
    nix develop --accept-flake-config .#android -c pnpm --dir js install
    nix develop --accept-flake-config .#android -c scripts/generate-sdk-rn-bindings.sh

# Regenerate, then build the two React Native packages' JavaScript (what CI's Build Android does).
build-rn-android: generate-sdk-rn-bindings
    nix develop --accept-flake-config .#android -c pnpm --dir js run build:reactnative

# iOS: cargo cross-compiles rust/fedimint-sdk inside the `.#ios` shell (macOS with Xcode only),
# then ubrn assembles the xcframework from those libraries and regenerates the bindings, building
# nothing itself (scripts/build-sdk-rn-ios.sh). UBRN_IOS_TARGETS (comma separated) narrows the
# slices; CI passes aarch64-apple-ios on pull requests. NIX_CONFIG serialises the build so rocksdb
# and aws-lc-sys do not exhaust macos-latest's memory.
build-rn-ios:
    nix develop --accept-flake-config .#ios -c pnpm --dir js install
    NIX_CONFIG=$'max-jobs = 1\ncores = 1' \
      nix develop --accept-flake-config .#ios -c scripts/build-sdk-rn-ios.sh
    nix develop --accept-flake-config .#ios -c pnpm --dir js run build:reactnative

# The xcframework @fedimint/react-native-bindings is published with, assembled from the Apple
# libraries `just build-ios-lib-nix` leaves behind. macOS with Xcode only; see the script.
assemble-rn-ios-xcframework:
    nix shell --accept-flake-config .#llvm-bitcode-strip -c scripts/assemble-rn-ios-xcframework.sh

test-coverage:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test:coverage

test-ui:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test:ui

# Stand up a devimint federation of the given module shape (v1, v2 or mixed) and
# run the fedimint-sdk integration tests against it.
test-sdk shape="v1":
    nix develop --accept-flake-config .#wasm-tests -c scripts/run-sdk-integration-tests.sh {{shape}}
