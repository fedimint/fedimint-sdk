set shell := ["bash", "-c"]

# Build Android bindings using Nix-cached Rust derivations.
# `ubrn build android --no-cargo` finds pre-placed .so files in the cargo
# target dir and skips the cross-compile.
build-android:
    nix develop --accept-flake-config -c pnpm --dir js install --frozen-lockfile
    nix develop --accept-flake-config .#android -c pnpm --dir js --filter @fedimint/react-native-bindings run ubrn:nix:android:release
    nix develop --accept-flake-config -c pnpm --dir js run build:reactnative

release-android: build-android

# Build iOS bindings using Nix-cached Rust derivations. macOS only.
# Requires the Nix daemon to permit `__noChroot` sandboxing
# (e.g. `--option sandbox relaxed`) so the iOS derivations can read Xcode.
#
# `NIX_CONFIG` serialises the iOS path end-to-end: Nix builds one
# derivation at a time (`max-jobs = 1`) and rustc/cc inside each
# derivation use a single thread (`cores = 1`). Heavy native deps
# (rocksdb, aws-lc-sys) otherwise saturate macos-latest's 7 GB RAM and
# OOM-kill the linker. Slower wall clock but the run actually finishes.
build-ios:
    nix develop --accept-flake-config -c pnpm --dir js install --frozen-lockfile
    NIX_CONFIG=$'max-jobs = 1\ncores = 1' nix develop --accept-flake-config .#ios -c pnpm --dir js --filter @fedimint/react-native-bindings run ubrn:nix:ios:release
    nix develop --accept-flake-config -c pnpm --dir js run build:reactnative

release-ios: build-ios

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
