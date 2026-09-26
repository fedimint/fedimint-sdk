#!/usr/bin/env bash
#
# Fetches the Apple native libraries from Nix (Cachix when warm) and drops them
# where the rest of the iOS pipeline expects them.
#
#   scripts/nix-build-ios-lib.sh
#     -> rust/fedimint-sdk/target/<triple>/release/libfedimint_sdk.a
#     -> rust/fedimint-sdk/target/lipo-ios-sim/release/libfedimint_sdk.a
#     -> rust/fedimint-sdk/target/apple-slices.txt
#
# This is the Nix counterpart of scripts/build-ios-lib.sh and produces byte-for-
# byte the same tree, including the manifest. That contract is deliberate:
# generate-swift-bindings.sh and build-ios-sdk.sh both key off
# `apple-slices.txt` to know which archives are fresh, and neither should have
# to care which producer made them.
#
# Apple targets build against an SDK that ships inside Xcode and cannot live in
# the nix store — but that only rules out a *pure* derivation, not a cacheable
# one. See the header of nix/ffi.nix for how `__noChroot` makes these ordinary,
# substitutable store paths, and for the one soundness caveat that comes with it.
#
# Nothing compiles on this machine when the Cachix cache is warm; a cold run
# cross-compiles the crate (heavy: rocksdb + aws-lc from C).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/rust/fedimint-sdk"
TARGET_DIR="$CRATE_DIR/target"
LIB_NAME="libfedimint_sdk.a"

# Keep in step with DEFAULT_TARGETS in scripts/build-ios-lib.sh: the two
# producers must be able to stand in for each other.
DEFAULT_TARGETS="aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin"
TARGETS="${IOS_TARGETS:-$DEFAULT_TARGETS}"

if [[ -n "${1:-}" ]]; then
    echo "usage: $0" >&2
    exit 1
fi

SIM_OUT="$TARGET_DIR/lipo-ios-sim/release"
MANIFEST="$TARGET_DIR/apple-slices.txt"

# Both are derived, and neither may outlive the inputs it came from. Removed
# up front and rewritten only on success, exactly as build-ios-lib.sh does.
rm -f "$SIM_OUT/$LIB_NAME"
rm -f "$MANIFEST"

built=()
for triple in $TARGETS; do
    echo "==> nix build .#fedimint-sdk-ios-$triple"
    out="$(nix build "$ROOT#fedimint-sdk-ios-$triple" \
        --accept-flake-config --no-link --print-out-paths)"

    src="$out/lib/$LIB_NAME"
    [[ -f "$src" ]] || {
        echo "no $LIB_NAME in $out for $triple" >&2
        exit 1
    }

    dest="$TARGET_DIR/$triple/release"
    mkdir -p "$dest"
    cp -L "$src" "$dest/$LIB_NAME"
    # Nix store outputs are read-only; a later run has to be able to replace it.
    chmod u+w "$dest/$LIB_NAME"

    built+=("$triple")
done

# Gated on `built` rather than on the files being present, for the same reason
# build-ios-lib.sh is: a previous full build leaves both simulator archives on
# disk, and merging one this run did not fetch would ship stale machine code.
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
    echo "==> no simulator target fetched this run, skipping lipo"
fi

mkdir -p "$(dirname "$MANIFEST")"
printf '%s\n' "${built[@]}" >"$MANIFEST"

echo "==> Done. Fetched: ${built[*]}"
for triple in "${built[@]}"; do
    printf '    %-24s %s\n' "$triple" \
        "$(du -h "$TARGET_DIR/$triple/release/$LIB_NAME" | cut -f1)"
done
