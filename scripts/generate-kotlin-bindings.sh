#!/usr/bin/env bash
#
# Generates the Kotlin bindings from an *already built* Android native library.
#
#   scripts/generate-kotlin-bindings.sh [jniLibs-dir]
#     -> android/fedimint-sdk/src/main/java/org/fedimint/sdk/fedimint_sdk.kt
#
# `uniffi-bindgen` reads the UniFFI metadata baked into the arm64 `.so` rather
# than the crate source, so the Kotlin can never drift from the binary it will
# load at runtime. That is also why this takes the `.so` as input instead of
# building one: the native library is built once
# (scripts/nix-build-android-so.sh, or .github/workflows/android-native.yaml)
# and every binding generator reads that same artifact.
#
# The generator is its own crate (rust/uniffi-bindgen) whose only dependency is
# `uniffi`, pinned to the version rust/fedimint-sdk links. That pin is load
# bearing: a mismatched reader does not fail cleanly, it walks the metadata
# with the wrong layout and dies partway through a record. Because the only
# dependency is uniffi, plain cargo builds it in about a minute — this
# deliberately does not go through Nix, so the Kotlin half needs no Nix, no
# NDK and no cross-compile toolchain at all. (`.#fedimint-uniffi-bindgen`
# exists in nix/ffi.nix for the same binary, but crane's vendoring loses
# uniffi_bindgen's askama.toml and the derivation does not currently build.)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JNI_LIBS="${1:-$ROOT/android/fedimint-sdk/src/main/jniLibs}"
LIB="$JNI_LIBS/arm64-v8a/libfedimint_sdk.so"
JAVA_OUT="$ROOT/android/fedimint-sdk/src/main/java"
# uniffi-bindgen nests output by package path, so uniffi.toml's package_name of
# org.fedimint.sdk lands here.
GEN_PKG_DIR="org/fedimint/sdk"

if [[ ! -f "$LIB" ]]; then
    echo "no native library at $LIB" >&2
    echo "build one first: ./scripts/nix-build-android-so.sh" >&2
    exit 1
fi

MERGED_CONFIG="$(mktemp)"
GEN_TMP="$(mktemp -d)"
trap 'rm -f "$MERGED_CONFIG"; rm -rf "$GEN_TMP"' EXIT

# uniffi.toml supplies this crate's [bindings.kotlin] package_name and
# cdylib_name; uniffi-android.toml supplies [defaults.kotlin] (android=true,
# android_cleaner=true), applied to every component. uniffi-bindgen auto-loads
# only uniffi.toml, so the two are concatenated into one --config here.
cat "$ROOT/rust/fedimint-sdk/uniffi.toml" \
    "$ROOT/rust/fedimint-sdk/uniffi-android.toml" >"$MERGED_CONFIG"

echo "==> Generating Kotlin from $LIB"
# Run from the crate directory: `uniffi`'s default `cargo-metadata` feature
# makes the generator shell out to `cargo metadata` for per-crate binding
# config, and that needs a manifest in the working directory. The repo root
# has none — this is a workspace of workspaces. Everything else here is an
# absolute path, so only the metadata lookup is affected.
cd "$ROOT/rust/fedimint-sdk"
cargo run --release \
    --manifest-path "$ROOT/rust/uniffi-bindgen/Cargo.toml" \
    -- generate \
    --library "$LIB" \
    --language kotlin \
    --config "$MERGED_CONFIG" \
    --no-format \
    --out-dir "$GEN_TMP"

# There is no hand-written Kotlin: the generated bindings are the API, because
# the exports hand out fedimint-sdk's real types rather than binding-only
# copies of them. The directory is therefore 100% generated and is wiped
# wholesale rather than merged into.
if [[ ! -d "$GEN_TMP/$GEN_PKG_DIR" ]]; then
    echo "uniffi-bindgen wrote nothing to $GEN_PKG_DIR — check uniffi.toml's package_name" >&2
    find "$GEN_TMP" -name '*.kt' >&2
    exit 1
fi

rm -rf "${JAVA_OUT:?}/$GEN_PKG_DIR"
mkdir -p "$JAVA_OUT/$GEN_PKG_DIR"
cp -R "$GEN_TMP/$GEN_PKG_DIR/." "$JAVA_OUT/$GEN_PKG_DIR/"

echo "==> Done."
find "$JAVA_OUT/$GEN_PKG_DIR" -name '*.kt' | sort
