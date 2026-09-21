#!/usr/bin/env bash
#
# Cross-compiles the Apple native libraries and nothing else — no bindings of
# any language.
#
#   scripts/build-ios-lib.sh
#     -> rust/fedimint-sdk/target/<triple>/release/libfedimint_sdk.a
#     -> rust/fedimint-sdk/target/lipo-ios-sim/release/libfedimint_sdk.a
#
# This stands on its own for the same reason scripts/nix-build-android-so.sh
# does: the native library is the input every binding generator shares.
# generate-swift-bindings.sh reads the UniFFI metadata straight out of the `.a`
# built here, and any other Apple generator added later reads the same one. It
# also means iterating on the bindings needs no native rebuild.
#
# Unlike Android, this does NOT go through Nix. Every Apple target compiles
# against an SDK that ships inside Xcode and cannot live in the nix store, so
# there is nothing to make a cacheable derivation out of — nix/ffi.nix stays
# Android-only. `nix develop .#ios` supplies the Rust targets and the C build
# tools; Xcode supplies the SDKs, reached through `xcrun`. This is the same
# shape js/react-native/react-native-bindings already uses for these three iOS
# triples.
#
# IOS_TARGETS overrides which triples are built. CI passes a subset on pull
# requests — a cold build of rocksdb and aws-lc from C, four times over, is the
# long pole — following the precedent in
# js/react-native/react-native-bindings/scripts/nix-prebuild.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/rust/fedimint-sdk"
TARGET_DIR="$CRATE_DIR/target"
LIB_NAME="libfedimint_sdk.a"

# `aarch64-apple-darwin` is not a mistake: the XCFramework carries a macOS slice
# so `swift test --package-path ios` runs on the host with no simulator boot.
# Keep in sync with iosToolchain in flake.nix and with the slice list in
# scripts/build-ios-sdk.sh.
DEFAULT_TARGETS="aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin"
TARGETS="${IOS_TARGETS:-$DEFAULT_TARGETS}"

# This script owns the default list and the `IOS_TARGETS` override, and
# `--print-targets` is how scripts/build-ios-sdk.sh asks what a run would build
# without having to repeat the default. Keeping one owner is what stops the
# orchestrator and the builder from silently disagreeing about which slices this
# invocation is responsible for.
if [[ "${1:-}" == "--print-targets" ]]; then
    echo "$TARGETS"
    exit 0
elif [[ -n "${1:-}" ]]; then
    echo "usage: $0 [--print-targets]" >&2
    exit 1
fi

# Must agree with ios/Package.swift's `platforms:`. cargo sets the Rust half per
# target, but the `cc` and `cmake` crates compiling rocksdb's and aws-lc's C and
# C++ sources read these from the environment — without them every object file
# is built for a different minimum version than the Swift that links it, and the
# linker warns once per object.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"

command -v xcrun >/dev/null || {
    echo "xcrun not found — this needs a macOS host with Xcode installed" >&2
    exit 1
}

for sdk in iphoneos iphonesimulator macosx; do
    xcrun --sdk "$sdk" --show-sdk-path >/dev/null 2>&1 || {
        echo "no $sdk SDK — check 'xcode-select -p' and 'xcodebuild -checkFirstLaunchStatus'" >&2
        exit 1
    }
done

# Fail here rather than partway through a long build. `--print target-list`
# would not do: it lists every target rustc knows about, installed or not.
# `target-libdir` is the directory the standard library for that target has to
# be in, so its absence is exactly "the std is missing".
for triple in $TARGETS; do
    libdir="$(rustc --print target-libdir --target "$triple" 2>/dev/null || true)"
    if [[ -z "$libdir" || ! -d "$libdir" ]]; then
        echo "no Rust standard library for $triple" >&2
        echo "install it with 'rustup target add $triple', or run inside 'nix develop .#ios'" >&2
        exit 1
    fi
done

# The lipo output is derived from the simulator slices, so a leftover from an
# earlier run must not survive into this one. Without this, a build that
# produces no simulator slice at all still leaves a stale fat archive sitting
# where build-ios-sdk.sh looks for one, and it gets packaged.
SIM_OUT="$TARGET_DIR/lipo-ios-sim/release"
rm -f "$SIM_OUT/$LIB_NAME"

built=()
for triple in $TARGETS; do
    echo "==> cargo build --release --features uniffi --target $triple"
    (cd "$CRATE_DIR" && cargo build \
        --locked \
        --release \
        --lib \
        --features uniffi \
        --target "$triple")

    out="$TARGET_DIR/$triple/release/$LIB_NAME"
    [[ -f "$out" ]] || {
        echo "no $LIB_NAME at $out after building $triple" >&2
        echo "check that [lib] crate-type in rust/fedimint-sdk/Cargo.toml still has \"staticlib\"" >&2
        exit 1
    }
    built+=("$triple")
done

# ---------------------------------------------------------------------------
# Merge the simulator slices
# ---------------------------------------------------------------------------
#
# An XCFramework allows at most one slice per (platform, variant), so the Apple
# Silicon and Intel simulator libraries have to become a single fat archive
# before xcodebuild will take them. `lipo -create` over one input is a plain
# copy, which is what a CI subset build ends up doing.

# Gated on `built`, not on the file being present: a previous full build leaves
# both simulator archives on disk, so probing the filesystem would merge a slice
# this run did not produce and ship stale machine code inside a fat archive that
# looks freshly made.
sim_inputs=()
for triple in aarch64-apple-ios-sim x86_64-apple-ios; do
    for done_triple in "${built[@]}"; do
        if [[ "$done_triple" == "$triple" ]]; then
            sim_inputs+=("$TARGET_DIR/$triple/release/$LIB_NAME")
            break
        fi
    done
done

if (( ${#sim_inputs[@]} > 0 )); then
    mkdir -p "$SIM_OUT"
    echo "==> lipo -create -> $SIM_OUT/$LIB_NAME"
    lipo -create "${sim_inputs[@]}" -output "$SIM_OUT/$LIB_NAME"
    lipo -info "$SIM_OUT/$LIB_NAME"
else
    echo "==> no simulator target built this run, skipping lipo"
fi

echo "==> Done. Built: ${built[*]}"
for triple in "${built[@]}"; do
    printf '    %-24s %s\n' "$triple" \
        "$(du -h "$TARGET_DIR/$triple/release/$LIB_NAME" | cut -f1)"
done
