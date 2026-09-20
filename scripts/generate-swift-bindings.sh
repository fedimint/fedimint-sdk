#!/usr/bin/env bash
#
# Generates the Swift bindings from an *already built* Apple native library.
#
#   scripts/generate-swift-bindings.sh [static-lib]
#     -> ios/Sources/FedimintSdk/FedimintSdk.swift
#     -> ios/Frameworks/Headers/FedimintSdkFFI.h
#     -> ios/Frameworks/Headers/module.modulemap
#
# `uniffi-bindgen` reads the UniFFI metadata baked into the library rather than
# the crate source, so the Swift can never drift from the binary it will load at
# runtime. That is also why this takes the library as input instead of building
# one: the native half is built once (scripts/build-ios-lib.sh) and every
# binding generator reads that same artifact. uniffi_bindgen handles a static
# archive directly — `Object::Archive` is one of the shapes its metadata
# extractor dispatches on — so this reads the real `.a` that ends up in the
# XCFramework, exactly as generate-kotlin-bindings.sh reads the arm64 `.so`.
#
# The generator is its own crate (rust/uniffi-bindgen) whose only dependency is
# `uniffi`, pinned to the version rust/fedimint-sdk links. That pin is load
# bearing: a mismatched reader does not fail cleanly, it walks the metadata with
# the wrong layout and dies partway through a record. Because the only
# dependency is uniffi, plain cargo builds it in about a minute — this
# deliberately does not go through Nix, so the Swift half needs no Nix at all.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/rust/fedimint-sdk"
TARGET_DIR="$CRATE_DIR/target"
SWIFT_OUT="$ROOT/ios/Sources/FedimintSdk"
HEADERS_OUT="$ROOT/ios/Frameworks/Headers"

# These three names all follow from uniffi.toml's [bindings.swift] module_name
# of FedimintSdk: uniffi derives the FFI module as "<module_name>FFI" and names
# the header and modulemap after it.
GEN_SWIFT="FedimintSdk.swift"
GEN_HEADER="FedimintSdkFFI.h"
GEN_MODULEMAP="FedimintSdkFFI.modulemap"

# Prefer the device library — it is the one a phone actually loads. The macOS
# slice is the fallback so a host-only build (what CI does on a pull request,
# and what anyone running `swift test` needs) still generates. The metadata is
# identical in every slice; only the machine code differs.
if [[ -n "${1:-}" ]]; then
    LIB="$1"
else
    LIB=""
    for triple in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin; do
        candidate="$TARGET_DIR/$triple/release/libfedimint_sdk.a"
        if [[ -f "$candidate" ]]; then
            LIB="$candidate"
            break
        fi
    done
fi

if [[ -z "$LIB" || ! -f "$LIB" ]]; then
    echo "no Apple native library found${LIB:+ at $LIB}" >&2
    echo "build one first: ./scripts/build-ios-lib.sh" >&2
    exit 1
fi

GEN_TMP="$(mktemp -d)"
trap 'rm -rf "$GEN_TMP"' EXIT

echo "==> Generating Swift from $LIB"
# Run from the crate directory: `uniffi`'s default `cargo-metadata` feature
# makes the generator shell out to `cargo metadata` for per-crate binding
# config, and that needs a manifest in the working directory. The repo root has
# none — this is a workspace of workspaces. Everything else here is an absolute
# path, so only the metadata lookup is affected.
#
# No --config concatenation, unlike the Kotlin script: uniffi.toml is
# auto-loaded and carries the whole [bindings.swift] table, and there is no
# Swift analogue of uniffi-android.toml.
#
# --xcframework is deliberately NOT passed. It switches the modulemap to the
# `framework module` form, which is for a real .framework bundle; a
# static-library XCFramework (`-library ... -headers ...`) needs the plain
# `module` form.
cd "$CRATE_DIR"
cargo run --release \
    --manifest-path "$ROOT/rust/uniffi-bindgen/Cargo.toml" \
    -- generate \
    --library "$LIB" \
    --language swift \
    --no-format \
    --out-dir "$GEN_TMP"

for f in "$GEN_SWIFT" "$GEN_HEADER" "$GEN_MODULEMAP"; do
    [[ -f "$GEN_TMP/$f" ]] || {
        echo "uniffi-bindgen did not write $f — check uniffi.toml's [bindings.swift] module_name" >&2
        find "$GEN_TMP" -type f >&2
        exit 1
    }
done

# There is no hand-written Swift API: the generated bindings are the API,
# because the exports hand out fedimint-sdk's real types rather than
# binding-only copies of them. FedimintSdk.swift is therefore replaced
# wholesale rather than merged into. Version.swift is the one committed file in
# this directory and is deliberately left alone — see its own comment for why
# it has to exist.
mkdir -p "$SWIFT_OUT" "$HEADERS_OUT"
rm -f "$SWIFT_OUT/$GEN_SWIFT"
cp "$GEN_TMP/$GEN_SWIFT" "$SWIFT_OUT/$GEN_SWIFT"

# The header and modulemap belong with the XCFramework, not with the Swift:
# `xcodebuild -create-xcframework -headers` copies this directory into every
# slice, and clang will only find the module if the file is named exactly
# `module.modulemap` there.
rm -f "$HEADERS_OUT"/*.h "$HEADERS_OUT"/module.modulemap
cp "$GEN_TMP/$GEN_HEADER" "$HEADERS_OUT/$GEN_HEADER"
cp "$GEN_TMP/$GEN_MODULEMAP" "$HEADERS_OUT/module.modulemap"

# uniffi.toml asks for these; without them the app fails to link with undefined
# symbols from netdev and socket2. Cheap to assert, miserable to diagnose later.
for framework in SystemConfiguration Security Network; do
    grep -q "link framework \"$framework\"" "$HEADERS_OUT/module.modulemap" || {
        echo "module.modulemap is missing 'link framework \"$framework\"'" >&2
        echo "check link_frameworks in rust/fedimint-sdk/uniffi.toml" >&2
        exit 1
    }
done

echo "==> Done."
printf '    %s\n' \
    "$SWIFT_OUT/$GEN_SWIFT" \
    "$HEADERS_OUT/$GEN_HEADER" \
    "$HEADERS_OUT/module.modulemap"
