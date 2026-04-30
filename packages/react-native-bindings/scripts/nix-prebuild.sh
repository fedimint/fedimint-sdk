#!/usr/bin/env bash
# Pre-populate the fedimint-sdk-ffi cargo target directory with Nix-built
# native libraries so `ubrn build <platform> --no-cargo` can pick them up
# instead of running cargo cross-compile from scratch.
#
# Usage:
#   nix-prebuild.sh android   # or: ios
#
# Requires the `fedimint-sdk-ffi` submodule to be checked out.
set -euo pipefail

PLATFORM="${1:?usage: $0 (android|ios)}"
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TARGET_DIR="$REPO_ROOT/fedimint-sdk-ffi/fedimint-client-uniffi/target"

cd "$REPO_ROOT"

place() {
    local triple="$1" pkg="$2" libname="$3"
    local out
    out=$(nix build --accept-flake-config --no-link --print-out-paths ".#$pkg")
    mkdir -p "$TARGET_DIR/$triple/release"
    install -m 0644 "$out/lib/$libname" \
        "$TARGET_DIR/$triple/release/$libname"
    echo "  $triple  <- $out/lib/$libname"
}

case "$PLATFORM" in
    android)
        echo "Pre-building Android libs via Nix..."
        place aarch64-linux-android android-aarch64-linux-android libfedimint_client_uniffi.so
        place x86_64-linux-android   android-x86_64-linux-android   libfedimint_client_uniffi.so
        ;;
    ios)
        echo "Pre-building iOS libs via Nix..."
        place aarch64-apple-ios     ios-aarch64-apple-ios     libfedimint_client_uniffi.a
        place aarch64-apple-ios-sim ios-aarch64-apple-ios-sim libfedimint_client_uniffi.a
        place x86_64-apple-ios      ios-x86_64-apple-ios      libfedimint_client_uniffi.a
        ;;
    *)
        echo "unknown platform: $PLATFORM" >&2
        exit 1
        ;;
esac
