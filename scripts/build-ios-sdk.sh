#!/usr/bin/env bash
#
# Builds the whole iOS SDK payload and drops it into the Swift package:
#
#   -> ios/Sources/FedimintSdk/FedimintSdk.swift
#   -> ios/Frameworks/FedimintSdkFFI.xcframework/
#
# Both are gitignored: they are regenerated, not committed. There is no
# hand-written Swift API — the generated bindings are the whole surface,
# because the crate's `#[uniffi::export]`s hand out fedimint-sdk's real types
# rather than binding-only copies of them (see rust/uniffi-bindgen/DECISION.md).
# ios/Sources/FedimintSdk/Version.swift is the single committed exception, and
# says so itself.
#
#   scripts/build-ios-sdk.sh    build-ios-lib.sh, then
#                               generate-swift-bindings.sh, then assemble the
#                               XCFramework out of whichever slices were built
#
# Only the XCFramework assembly lives here; everything else is delegated, so a
# local build and CI can never run different bindgen invocations.
#
# Needs a macOS host with Xcode, the Apple Rust targets, and the cmake/perl/go
# that aws-lc-sys's and rocksdb's C sources want — `nix develop .#ios` supplies
# all of that except Xcode itself.
#
# IOS_TARGETS is forwarded to build-ios-lib.sh; the XCFramework is assembled
# from whatever that produced, so a subset build yields a subset framework.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT/rust/fedimint-sdk/target"
HEADERS="$ROOT/ios/Frameworks/Headers"
XCFRAMEWORK="$ROOT/ios/Frameworks/FedimintSdkFFI.xcframework"
LIB_NAME="libfedimint_sdk.a"

"$ROOT/scripts/build-ios-lib.sh"
"$ROOT/scripts/generate-swift-bindings.sh"

# ---------------------------------------------------------------------------
# XCFramework
# ---------------------------------------------------------------------------
#
# One slice per (platform, variant). The simulator slice is the fat archive
# build-ios-lib.sh lipo'd; the other two are single-architecture.
#
# `-headers` copies the whole directory into every slice, which is why
# generate-swift-bindings.sh puts the header and `module.modulemap` there and
# nothing else.

args=()
slices=()
add_slice() {
    local label="$1" lib="$2"
    if [[ -f "$lib" ]]; then
        args+=(-library "$lib" -headers "$HEADERS")
        slices+=("$label")
    fi
}

add_slice "ios-arm64"                  "$TARGET_DIR/aarch64-apple-ios/release/$LIB_NAME"
add_slice "ios-arm64_x86_64-simulator" "$TARGET_DIR/lipo-ios-sim/release/$LIB_NAME"
add_slice "macos-arm64"                "$TARGET_DIR/aarch64-apple-darwin/release/$LIB_NAME"

if (( ${#slices[@]} == 0 )); then
    echo "no built libraries to assemble — did build-ios-lib.sh run?" >&2
    exit 1
fi

# -create-xcframework refuses to write over an existing bundle.
rm -rf "$XCFRAMEWORK"

echo "==> xcodebuild -create-xcframework (${slices[*]})"
xcodebuild -create-xcframework "${args[@]}" -output "$XCFRAMEWORK"

# Read the result back rather than trusting the exit code: a slice whose
# modulemap failed to copy still produces a bundle, and the failure only shows
# up much later as "no such module 'FedimintSdkFFI'" from swiftc.
for dir in "$XCFRAMEWORK"/*/; do
    [[ -d "$dir" ]] || continue
    [[ -f "$dir/Headers/module.modulemap" ]] || {
        echo "slice $(basename "$dir") has no Headers/module.modulemap" >&2
        exit 1
    }
done

# Report what the bundle actually contains rather than what was asked for:
# xcodebuild names each slice after the architectures it really found, so a
# simulator library with only the arm64 half lands as `ios-arm64-simulator`,
# not `ios-arm64_x86_64-simulator`.
echo "==> Done."
echo "    $(basename "$XCFRAMEWORK")"
for dir in "$XCFRAMEWORK"/*/; do
    [[ -d "$dir" ]] || continue
    printf '      %-32s %s\n' "$(basename "$dir")" \
        "$(lipo -archs "$dir/$LIB_NAME" 2>/dev/null)"
done
