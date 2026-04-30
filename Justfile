set shell := ["bash", "-c"]

# Ensure the fedimint-sdk-ffi submodule is checked out (Rust crate metadata
# is read by ubrn even when --no-cargo skips the build).
clone-ffi:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d "fedimint-sdk-ffi/fedimint-client-uniffi" ]; then
        echo "fedimint-sdk-ffi not present; initialising submodule…"
        git submodule update --init --recursive
    fi

# Build Android bindings using Nix-cached Rust derivations.
# `ubrn build android --no-cargo` finds pre-placed .so files in the cargo
# target dir and skips the cross-compile.
build-android: clone-ffi
    nix develop --accept-flake-config -c pnpm install --frozen-lockfile
    nix develop --accept-flake-config .#android -c pnpm --filter @fedimint/react-native-bindings run ubrn:nix:android:release
    nix develop --accept-flake-config -c pnpm run build:reactnative

release-android: build-android

# Build iOS bindings using Nix-cached Rust derivations. macOS only.
# Requires the Nix daemon to permit `__noChroot` sandboxing
# (e.g. `--option sandbox relaxed`) so the iOS derivations can read Xcode.
build-ios: clone-ffi
    nix develop --accept-flake-config -c pnpm install --frozen-lockfile
    nix develop --accept-flake-config .#ios -c pnpm --filter @fedimint/react-native-bindings run ubrn:nix:ios:release
    nix develop --accept-flake-config -c pnpm run build:reactnative

release-ios: build-ios

test:
    nix develop --accept-flake-config .#wasm-tests -c pnpm run test

test-coverage:
    nix develop --accept-flake-config .#wasm-tests -c pnpm run test:coverage

test-ui:
    nix develop --accept-flake-config .#wasm-tests -c pnpm run test:ui
