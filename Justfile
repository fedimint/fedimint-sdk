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

# Compile the library and the example app against the freshly generated bindings.
test-kotlin: build-kotlin
    cd android && ./gradlew :fedimint-sdk:assembleRelease :app:assembleDebug

# The example APK the E2E suite installs: the native library, the Kotlin generated
# from it, then Gradle, all in the lean `.#android` shell (Android SDK and a
# JDK — no emulator, no devimint). CI's android-apk.yaml is this on its own
# machine. It is a separate step from `test-android-e2e` on purpose: a Gradle
# build alongside an emulator and a devimint federation starves the emulator
# until Android's System UI stops responding, so the build finishes — daemon
# and all — before either of those starts.
build-android-apk:
    nix develop --accept-flake-config .#android -c bash -c './scripts/build-android-sdk.sh && cd android && ./gradlew :app:assembleDebug'

# Boot an emulator, install the example app and drive it with the Appium suite
# (js/android/integration-tests) against a devimint federation — the Android
# counterpart of `just test`, which does the same for the wasm client, through
# the same scripts/setup_test_shell.sh. There is deliberately no federation-free
# variant: one way to run this suite, so what CI does and what you can
# reproduce are the same thing. Builds the APK first (see build-android-apk);
# the script itself only installs one.
test-android-e2e: build-android-apk
    nix develop --accept-flake-config .#android-tests -c scripts/setup_test_shell.sh bash scripts/e2e-android/run-android-e2e.sh

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
