set shell := ["bash", "-c"]

# Native libraries only (.so). Shared by every binding generator.
build-android-so:
    ./scripts/nix-build-android-so.sh

# The Kotlin bindings, read out of an already-built .so.
build-kotlin-bindings:
    ./scripts/generate-kotlin-bindings.sh

# Both halves: native libraries, then the Kotlin generated from them.
build-kotlin:
    ./scripts/nix-build-kotlin.sh

# Compile the library and the demo app against the freshly generated bindings.
test-kotlin: build-kotlin
    cd android && ./gradlew :fedimint-sdk:assembleRelease :app:assembleDebug

# Assemble the release AAR (publishing is not wired up yet).
build-android-aar: build-kotlin
    cd android && ./gradlew :fedimint-sdk:assembleRelease

# Non-nix escape hatch: cross-compile + generate locally with cargo-ndk.
# Needs the `.#android` shell (NDK, cargo-ndk, cmake/go for aws-lc-sys).
build-android-local:
    nix develop --accept-flake-config .#android -c ./scripts/generate-android-so.sh

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
