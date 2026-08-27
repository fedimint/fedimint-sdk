#!/usr/bin/env bash
# Pre-populate the fedimint-client-uniffi cargo target directory with
# Nix-built native libraries so `ubrn build <platform> --no-cargo` can pick
# them up instead of running cargo cross-compile from scratch.
#
# Usage:
#   nix-prebuild.sh android   # or: ios
set -euo pipefail

PLATFORM="${1:?usage: $0 (android|ios)}"
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TARGET_DIR="$REPO_ROOT/rust/fedimint-client-uniffi/target"

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
        # IOS_TARGETS controls which iOS Rust targets to pre-build. Defaults
        # to all three (device + arm64 sim + x86_64 sim) so a release build
        # produces a complete .xcframework. PR validation passes
        # IOS_TARGETS="aarch64-apple-ios" to smoke-test only the device
        # slice -- skipping the two simulator targets roughly halves the
        # iOS Rust cross-compile time on macos-latest.
        IOS_TARGETS="${IOS_TARGETS:-aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios}"
        echo "Pre-building iOS libs via Nix (targets: $IOS_TARGETS)..."
        for triple in $IOS_TARGETS; do
            case "$triple" in
                aarch64-apple-ios)     place "$triple" ios-aarch64-apple-ios     libfedimint_client_uniffi.a ;;
                aarch64-apple-ios-sim) place "$triple" ios-aarch64-apple-ios-sim libfedimint_client_uniffi.a ;;
                x86_64-apple-ios)      place "$triple" ios-x86_64-apple-ios      libfedimint_client_uniffi.a ;;
                *)                     echo "unknown iOS target: $triple" >&2; exit 1 ;;
            esac
        done
        ;;
    *)
        echo "unknown platform: $PLATFORM" >&2
        exit 1
        ;;
esac
