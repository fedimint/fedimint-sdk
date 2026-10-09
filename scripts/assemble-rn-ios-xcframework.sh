#!/usr/bin/env bash
#
# Assembles the iOS xcframework that @fedimint/react-native-bindings ships.
#
#   scripts/assemble-rn-ios-xcframework.sh
#     -> js/react-native/react-native-bindings/FedimintReactNativeBindingsFramework.xcframework/
#
# macOS with Xcode only. It needs `xcodebuild`, `lipo` and `otool` (all three come with Xcode) and
# `llvm-bitcode-strip`, which Xcode does not provide but the flake does:
#
#   nix shell .#llvm-bitcode-strip -c scripts/assemble-rn-ios-xcframework.sh
#
# The bundle is what the package's podspec (ReactNativeBindings.podspec) vendors, in the layout
# `ubrn build ios` produces and the podspec expects:
#
#   FedimintReactNativeBindingsFramework.xcframework/
#     Info.plist
#     ios-arm64/libfedimint_sdk.a
#     ios-arm64_x86_64-simulator/libfedimint_sdk.a
#
# It has no Headers directory, because the React Native C++ declares the library's functions
# itself, and no macOS slice, because React Native does not run there.
#
# The libraries are not compiled here. They are the ones scripts/nix-build-ios-lib.sh leaves in
# rust/fedimint-sdk/target/<triple>/release/ for the Swift SDK. All three iOS triples are required,
# since the package always ships the device and both simulator architectures. The manifest
# target/apple-slices.txt lists the triples that script produced in its last run; it is what
# tells a library from this run from one left over by an earlier run, so it is checked as well as
# the files.
#
# The Swift SDK's own bundle (scripts/assemble-ios-xcframework.sh) is not reused because it has
# another name, carries a macOS slice the package has no use for, and has a Headers directory with
# a module map that the React Native C++ never includes. So the same libraries are assembled a
# second time, here. Nothing in target/ is modified, as the Swift SDK is assembled from the same
# files.
#
# One thing is done to the libraries on the way: the LLVM bitcode that rustc embeds in every
# object (the sections __LLVM,__bitcode and __LLVM,__cmdline) is removed. Nothing reads it, as
# Xcode no longer accepts bitcode and this one is Rust's LLVM version anyway. It is 61% of each
# archive: 354 MB with it and 137 MB without, 103 MB against 37 MB compressed, per architecture,
# and the package carries three. Removing the two sections leaves the machine code byte for byte
# the same and removes no exported symbol, only the local symbols that named the removed sections.
# The simulator slice is merged from the stripped libraries, not from the merged archive
# nix-build-ios-lib.sh makes, which has the bitcode.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT/rust/fedimint-sdk/target"
MANIFEST="$TARGET_DIR/apple-slices.txt"
BUNDLE_NAME="FedimintReactNativeBindingsFramework.xcframework"
BUNDLE="$ROOT/js/react-native/react-native-bindings/$BUNDLE_NAME"
LIB_NAME="libfedimint_sdk.a"
TRIPLES="aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios"
# The directory names xcodebuild gives the slices, after the architectures it finds in them.
DEVICE_SLICE="ios-arm64"
SIM_SLICE="ios-arm64_x86_64-simulator"

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Fails unless the program $1 is on PATH. $2 says where to get it.
need() {
    command -v "$1" >/dev/null || die "$1 is not on PATH; $2"
}

if (($# > 0)); then
    usage
fi

need xcodebuild "it comes with Xcode: install Xcode and select it with xcode-select."
need lipo "it comes with Xcode: install Xcode and select it with xcode-select."
need otool "it comes with Xcode: install Xcode and select it with xcode-select."
need llvm-bitcode-strip "run this script as: \
nix shell .#llvm-bitcode-strip -c scripts/assemble-rn-ios-xcframework.sh"

# ---------------------------------------------------------------------------
# Inputs
# ---------------------------------------------------------------------------

[[ -f "$MANIFEST" ]] ||
    die "no manifest at $MANIFEST;" \
        "produce the native libraries first: ./scripts/nix-build-ios-lib.sh"

missing=()
for triple in $TRIPLES; do
    grep -qxF "$triple" "$MANIFEST" || missing+=("$triple")
done
if ((${#missing[@]} > 0)); then
    die "$MANIFEST does not list: ${missing[*]}. All of $TRIPLES are required;" \
        "produce them: ./scripts/nix-build-ios-lib.sh"
fi

for triple in $TRIPLES; do
    [[ -f "$TARGET_DIR/$triple/release/$LIB_NAME" ]] ||
        die "$triple is listed in $MANIFEST but" \
            "$TARGET_DIR/$triple/release/$LIB_NAME is missing;" \
            "produce it: ./scripts/nix-build-ios-lib.sh"
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ---------------------------------------------------------------------------
# Strip the bitcode, merge the simulators
# ---------------------------------------------------------------------------

for triple in $TRIPLES; do
    echo "==> llvm-bitcode-strip $triple"
    mkdir -p "$TMP/$triple"
    llvm-bitcode-strip -r "$TARGET_DIR/$triple/release/$LIB_NAME" -o "$TMP/$triple/$LIB_NAME"
done

echo "==> lipo -create (aarch64-apple-ios-sim, x86_64-apple-ios)"
mkdir -p "$TMP/ios-simulator"
lipo -create "$TMP/aarch64-apple-ios-sim/$LIB_NAME" "$TMP/x86_64-apple-ios/$LIB_NAME" \
    -output "$TMP/ios-simulator/$LIB_NAME"

# ---------------------------------------------------------------------------
# XCFramework
# ---------------------------------------------------------------------------

# -create-xcframework refuses to write over an existing bundle.
rm -rf "$BUNDLE"

echo "==> xcodebuild -create-xcframework"
xcodebuild -create-xcframework \
    -library "$TMP/aarch64-apple-ios/$LIB_NAME" \
    -library "$TMP/ios-simulator/$LIB_NAME" \
    -output "$BUNDLE"

# Read the result back rather than trusting the exit code. xcodebuild names a slice after the
# architectures it found, so a wrong input shows up as a different directory name, and it copies
# whatever else comes with a library (headers, for one).
for path in Info.plist "$DEVICE_SLICE/$LIB_NAME" "$SIM_SLICE/$LIB_NAME"; do
    [[ -f "$BUNDLE/$path" ]] ||
        die "$BUNDLE has no $path; it holds:" \
            "$(cd "$BUNDLE" && find . -mindepth 1 | sort | tr '\n' ' ')"
done

while IFS= read -r entry; do
    case "$entry" in
        ./Info.plist | ./"$DEVICE_SLICE" | ./"$DEVICE_SLICE/$LIB_NAME" | \
            ./"$SIM_SLICE" | ./"$SIM_SLICE/$LIB_NAME") ;;
        *) die "$BUNDLE has an unexpected entry, $entry; it must hold only Info.plist and" \
            "$DEVICE_SLICE and $SIM_SLICE, each with just $LIB_NAME." ;;
    esac
done < <(cd "$BUNDLE" && find . -mindepth 1 | sort)

# otool's output goes to a file: a pipe into `grep -q` would end the pipeline early, and with
# pipefail that reads as a failure of the match.
for slice in "$DEVICE_SLICE" "$SIM_SLICE"; do
    otool -l "$BUNDLE/$slice/$LIB_NAME" >"$TMP/otool.txt" ||
        die "otool failed on $BUNDLE/$slice/$LIB_NAME"
    if grep -q 'sectname __bitcode' "$TMP/otool.txt"; then
        die "$slice/$LIB_NAME still has bitcode (section __bitcode);" \
            "llvm-bitcode-strip did not remove it."
    fi
done

echo "==> Done."
echo "    $(basename "$BUNDLE")"
for slice in "$DEVICE_SLICE" "$SIM_SLICE"; do
    printf '      %-32s %-14s %s\n' "$slice" \
        "$(lipo -archs "$BUNDLE/$slice/$LIB_NAME")" \
        "$(du -h "$BUNDLE/$slice/$LIB_NAME" | cut -f1)"
done
