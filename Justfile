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

# Boot an emulator, install the demo app and drive it with the Appium suite
# (js/android/integration-tests). Builds the SDK payload first unless
# SKIP_BINDINGS_BUILD=true says it is already in place.
test-android-e2e:
    nix develop --accept-flake-config .#android-tests -c scripts/e2e-android/run-android-e2e.sh

# Assemble the release AAR (publishing is not wired up yet).
build-android-aar: build-kotlin
    cd android && ./gradlew :fedimint-sdk:assembleRelease

# Non-nix escape hatch: cross-compile + generate locally with cargo-ndk.
# Needs the `.#android` shell (NDK, cargo-ndk, cmake/go for aws-lc-sys).
build-android-local:
    nix develop --accept-flake-config .#android -c ./scripts/build-android-sdk.sh --local

test:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test

test-coverage:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test:coverage

test-ui:
    nix develop --accept-flake-config .#wasm-tests -c pnpm --dir js run test:ui

# Stand up a devimint federation of the given module shape (v1, v2 or mixed) and
# run the fedimint-sdk integration tests against it.
test-sdk shape="v1":
    nix develop --accept-flake-config .#wasm-tests -c scripts/run-sdk-integration-tests.sh {{shape}}
