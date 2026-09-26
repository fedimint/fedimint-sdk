#!/usr/bin/env bash
#
# Assembles the XCFramework from native libraries that are *already* on disk.
#
#   scripts/assemble-ios-xcframework.sh
#     -> ios/Frameworks/FedimintSdkFFI.xcframework/
#
# Split out from build-ios-sdk.sh so CI can assemble from slices downloaded as
# an artifact rather than re-running a producer: ios-native.yaml builds them
# once, and swift-sdk.yaml only puts them together. Locally, build-ios-sdk.sh
# calls this as its last step, so both paths assemble identically.
#
# Reads `apple-slices.txt` to know which slices are fresh — whichever producer
# wrote it. See build-ios-lib.sh for why that manifest exists.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT/rust/fedimint-sdk/target"
HEADERS="$ROOT/ios/Frameworks/Headers"
XCFRAMEWORK="$ROOT/ios/Frameworks/FedimintSdkFFI.xcframework"
LIB_NAME="libfedimint_sdk.a"

MANIFEST="$TARGET_DIR/apple-slices.txt"
[[ -f "$MANIFEST" ]] || {
    echo "no manifest at $MANIFEST" >&2
    echo "produce the native libraries first: ./scripts/nix-build-ios-lib.sh" >&2
    echo "(or ./scripts/build-ios-lib.sh for the plain-cargo path)" >&2
    exit 1
}

built_this_run() {
    grep -qxF "$1" "$MANIFEST"
}

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

# Each slice is gated on a triple from TARGETS rather than on `[[ -f ]]`, so a
# subset build produces a subset framework instead of quietly shipping whatever
# an earlier run left behind. The file check stays as a sanity assert: by this
# point the library is supposed to exist, and its absence is a bug worth
# shouting about rather than silently dropping a slice.
args=()
slices=()
add_slice() {
    local label="$1" lib="$2"
    shift 2
    local triple
    for triple in "$@"; do
        if built_this_run "$triple"; then
            [[ -f "$lib" ]] || {
                echo "built $triple this run but $lib is missing" >&2
                exit 1
            }
            args+=(-library "$lib" -headers "$HEADERS")
            slices+=("$label")
            return 0
        fi
    done
    return 0
}

add_slice "ios-arm64" "$TARGET_DIR/aarch64-apple-ios/release/$LIB_NAME" \
    aarch64-apple-ios
# Either simulator triple produces the lipo'd archive, so either one earns the slice.
add_slice "ios-arm64_x86_64-simulator" "$TARGET_DIR/lipo-ios-sim/release/$LIB_NAME" \
    aarch64-apple-ios-sim x86_64-apple-ios
add_slice "macos-arm64" "$TARGET_DIR/aarch64-apple-darwin/release/$LIB_NAME" \
    aarch64-apple-darwin

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
