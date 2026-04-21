set shell := ["bash", "-c"]

clone-ffi:
    #!/usr/bin/env bash
    if [ ! -d "fedimint-sdk-ffi" ]; then
        echo "fedimint-sdk-ffi not found. Cloning it now..."
        git submodule update --init --recursive
    else
        echo "fedimint-sdk-ffi already exists, skipping clone."
    fi

build-android: clone-ffi
    nix develop --accept-flake-config .#android -c pnpm i
    nix develop --accept-flake-config .#android -c pnpm run ubrn:android
    nix develop --accept-flake-config -c pnpm run build

release-android: clone-ffi
    nix develop --accept-flake-config .#android -c pnpm i
    nix develop --accept-flake-config .#android -c pnpm run ubrn:android:release
    nix develop --accept-flake-config -c pnpm run build

build-ios: clone-ffi
    nix develop --accept-flake-config .#ios -c pnpm i
    nix develop --accept-flake-config .#ios -c pnpm run ubrn:ios
    nix develop --accept-flake-config -c pnpm run build

release-ios: clone-ffi
    nix develop --accept-flake-config .#ios -c pnpm i
    nix develop --accept-flake-config .#ios -c pnpm run ubrn:ios:release
    nix develop --accept-flake-config -c pnpm run build