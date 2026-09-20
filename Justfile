set shell := ["bash", "-c"]

# Native libraries only (.so), via Nix. Shared by every binding generator,
# and what the two-job CI split (android-native.yaml, kotlin-sdk.yaml) uses
# so a binding job can read an artifact instead of rebuilding.
build-android-so:
    ./scripts/nix-build-android-so.sh

# The Kotlin bindings, read out of an already-built .so.
build-kotlin-bindings:
    ./scripts/generate-kotlin-bindings.sh

build-kotlin:
    ./scripts/build-android-sdk.sh

# Compile the library and the demo app against the freshly generated bindings.
test-kotlin: build-kotlin
    cd android && ./gradlew :fedimint-sdk:assembleRelease :app:assembleDebug

# Assemble the release AAR (publishing is not wired up yet).
build-android-aar: build-kotlin
    cd android && ./gradlew :fedimint-sdk:assembleRelease

# Non-nix escape hatch: cross-compile + generate locally with cargo-ndk.
# Needs the `.#android` shell (NDK, cargo-ndk, cmake/go for aws-lc-sys).
build-android-local:
    nix develop --accept-flake-config .#android -c ./scripts/build-android-sdk.sh --local

# Boot the emulator the React Native example apps run on (created on first use, see the
# script). Leave it running; pass emulator flags after the recipe name, e.g. -no-window.
android-emulator *ARGS:
    nix develop --accept-flake-config .#android-emulator -c scripts/android-emulator.sh {{ARGS}}

# Build and install one example app (react-native or expo-app) on the running emulator or a
# connected device and launch it, from the same shell as the emulator so one adb owns the device.
# Needs `just build-rn-android` first. See the script for why this is not `react-native
# run-android`.
rn-example app="react-native":
    nix develop --accept-flake-config .#android-emulator -c scripts/rn-example.sh {{app}}

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

# The Swift bindings, read out of an already-built .a.
build-swift-bindings:
    ./scripts/generate-swift-bindings.sh

build-swift:
    nix develop --accept-flake-config .#ios -c ./scripts/build-ios-sdk.sh

# Runs on the macOS slice of the XCFramework — no simulator boot needed.
test-swift: build-swift
    swift test --package-path ios

# Compile the demo app against the freshly generated bindings, the Swift
# counterpart of `test-kotlin`'s :app:assembleDebug. The .xcodeproj is
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
# ubrn assembles the xcframework and regenerates the bindings. UBRN_IOS_TARGETS (comma separated)
# narrows the slices; CI passes aarch64-apple-ios on pull requests. NIX_CONFIG serialises the
# build so rocksdb and aws-lc-sys do not exhaust macos-latest's memory.
build-rn-ios:
    nix develop --accept-flake-config .#ios -c pnpm --dir js install
    NIX_CONFIG=$'max-jobs = 1\ncores = 1' \
      nix develop --accept-flake-config .#ios -c scripts/build-sdk-rn-ios.sh
    nix develop --accept-flake-config .#ios -c pnpm --dir js run build:reactnative

test-coverage:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test:coverage

test-ui:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test:ui

# Stand up a devimint federation of the given module shape (v1, v2 or mixed) and
# run the fedimint-sdk integration tests against it.
test-sdk shape="v1":
    nix develop --accept-flake-config .#wasm-tests -c scripts/run-sdk-integration-tests.sh {{shape}}
