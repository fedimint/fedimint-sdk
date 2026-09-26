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
#   scripts/build-ios-sdk.sh            nix-build-ios-lib.sh, then
#                                        generate-swift-bindings.sh, then
#                                        assemble the XCFramework out of
#                                        whichever slices were produced
#   scripts/build-ios-sdk.sh --local    cross-compile locally with plain cargo
#                                        (build-ios-lib.sh) instead of Nix, for
#                                        a machine that cannot or should not
#                                        use it, then the same two steps
#
# The two producers are interchangeable: both write the per-triple archives, the
# lipo'd simulator slice and `apple-slices.txt`, so nothing downstream knows or
# cares which one ran. Mirrors scripts/build-android-sdk.sh and its `--local`.
#
# Only the XCFramework assembly lives here; everything else is delegated, so a
# local build and CI can never run different bindgen invocations.
#
# Needs a macOS host with Xcode. The default path additionally needs Nix; the
# --local path needs the Apple Rust targets and the cmake/perl/go that
# aws-lc-sys's and rocksdb's C sources want — `nix develop .#ios` supplies all
# of that except Xcode itself.
#
# IOS_TARGETS is forwarded to whichever producer runs; the XCFramework is
# assembled from what it produced, so a subset build yields a subset framework.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="$ROOT/rust/fedimint-sdk/target"
LIB_NAME="libfedimint_sdk.a"

MODE="nix"
if [[ "${1:-}" == "--local" ]]; then
    MODE="local"
elif [[ -n "${1:-}" ]]; then
    echo "usage: $0 [--local]" >&2
    exit 1
fi

if [[ "$MODE" == "nix" ]]; then
    "$ROOT/scripts/nix-build-ios-lib.sh"
else
    "$ROOT/scripts/build-ios-lib.sh"
fi

# Everything below keys off what that run *produced*, not off what is on disk: a
# populated target/ from an earlier full build would otherwise let a subset run
# package archives it did not produce. Both producers record the list, having
# deleted any previous one before starting, so this cannot outlive its build.
MANIFEST="$TARGET_DIR/apple-slices.txt"
[[ -f "$MANIFEST" ]] || {
    echo "the native build wrote no manifest at $MANIFEST" >&2
    exit 1
}
TARGETS="$(tr '\n' ' ' <"$MANIFEST")"

built_this_run() {
    grep -qxF "$1" "$MANIFEST"
}

# Generate from a slice this run built, chosen explicitly rather than left to
# the fallback in generate-swift-bindings.sh. Device first — it is the one a
# phone actually loads — then the simulators, then macOS. The metadata is
# identical in every slice; what matters is that it came from this build, which
# is also why every triple has to be listed: an Intel-simulator-only build
# carries the same metadata as any other and must not be rejected.
BINDINGS_LIB=""
for triple in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin; do
    if built_this_run "$triple"; then
        BINDINGS_LIB="$TARGET_DIR/$triple/release/$LIB_NAME"
        break
    fi
done
if [[ -z "$BINDINGS_LIB" ]]; then
    echo "none of the Apple targets that can carry UniFFI metadata were built" >&2
    echo "IOS_TARGETS was: $TARGETS" >&2
    exit 1
fi

"$ROOT/scripts/generate-swift-bindings.sh" "$BINDINGS_LIB"

"$ROOT/scripts/assemble-ios-xcframework.sh"
