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
# copies of them. The directory is therefore wholly produced by this script —
# uniffi's output plus the one patch below, applied the same way every run —
# and is wiped wholesale rather than merged into.
if [[ ! -d "$GEN_TMP/$GEN_PKG_DIR" ]]; then
    echo "uniffi-bindgen wrote nothing to $GEN_PKG_DIR — check uniffi.toml's package_name" >&2
    find "$GEN_TMP" -name '*.kt' >&2
    exit 1
fi

# ── Load the library through ART, so its JNI_OnLoad runs ─────────────────────
#
# uniffi's Kotlin loads the native library with JNA's `Native.register`, and
# JNA `dlopen`s it directly. A bare `dlopen` never calls the library's
# `JNI_OnLoad` — only ART's own loader does — and `JNI_OnLoad` is the only
# place rust/fedimint-sdk/src/android.rs is handed the JavaVM it needs to give
# the DNS resolver underneath iroh the platform's Context. Without it, the first dial to an
# `iroh://` gateway or guardian aborts the host app with "android context was
# not initialized". This inserts a `System.loadLibrary` ahead of each
# `Native.register`, so ART loads the library first and JNA's `dlopen` then
# finds that same copy already loaded — verified on a device: one copy, and
# `JNI_OnLoad` runs.
#
# It lives here rather than in every app that embeds the SDK because the
# failure it prevents does not show up until a federation advertises iroh, so
# an app that forgot it would pass its own testing and crash in the field.
#
# Applied in $GEN_TMP, before anything in the source tree is replaced, so a
# patch that cannot apply fails the script with the previous bindings intact.
# And it fails rather than skipping: an unpatched build compiles and works
# until the first iroh dial, which is exactly the failure this exists to
# prevent, so a uniffi upgrade that reshapes these lines has to be caught here.
ART_LOADER_HELPER="$(cat <<'KOTLIN'
// ---- Inserted by scripts/generate-kotlin-bindings.sh; not part of uniffi's output. ----
//
// Loads the native library through ART's own loader before JNA registers it. JNA `dlopen`s the
// library directly, and a bare `dlopen` never runs the library's `JNI_OnLoad` — the only place
// the Rust side is handed the JavaVM it needs to give the DNS resolver underneath iroh a Context.
// JNA's own load right after finds the library already loaded and reuses that copy.
//
// Best effort by design: if this fails, JNA's load that follows either succeeds, leaving things
// as they were before this existed, or fails with its own error. Either way the failure here is
// logged under the SDK's logcat tag rather than swallowed.
private fun uniffiLoadThroughArt(componentName: String) {
    val name = findLibraryName(componentName)
    try {
        // A bare library name unless a libraryOverride supplies a path, and only
        // `System.load` accepts a path.
        if (name.contains('/')) System.load(name) else System.loadLibrary(name)
    } catch (e: UnsatisfiedLinkError) {
        android.util.Log.w(
            "fedimint-sdk",
            "could not load $name through ART, so JNI_OnLoad will not run; falling back to JNA",
            e,
        )
    }
}
KOTLIN
)"

patched=0
while IFS= read -r kt; do
    registers="$(grep -c 'Native\.register(' "$kt" || true)"
    anchors="$(grep -c '^internal object IntegrityCheckingUniffiLib {$' "$kt" || true)"
    if [[ "$registers" != 2 || "$anchors" != 1 ]]; then
        echo "cannot patch $kt: expected 2 Native.register calls and 1 IntegrityCheckingUniffiLib" \
            "object, found $registers and $anchors" >&2
        echo "uniffi's generated loader has changed shape; update the patch in $0" >&2
        exit 1
    fi
    # The helper goes ahead of the first object that registers; a call to it
    # goes ahead of each registration, reusing that line's indentation and
    # component name so nothing here restates what uniffi generated.
    HELPER="$ART_LOADER_HELPER" perl -0777 -pi -e '
        s/^(internal object IntegrityCheckingUniffiLib \{)$/$ENV{HELPER}\n\n${1}/m;
        s/^([ \t]*)(Native\.register\(\w+::class\.java, findLibraryName\(componentName = "([^"]+)"\)\))$/${1}uniffiLoadThroughArt(componentName = "${3}")\n${1}${2}/mg;
    ' "$kt"
    calls="$(grep -c 'uniffiLoadThroughArt(componentName = "' "$kt" || true)"
    if [[ "$calls" != 2 ]]; then
        echo "patching $kt inserted $calls loader calls, expected 2" >&2
        exit 1
    fi
    patched=$((patched + 1))
done < <(grep -l 'Native\.register(' "$GEN_TMP/$GEN_PKG_DIR"/*.kt)
if [[ "$patched" == 0 ]]; then
    echo "found no generated loader to patch under $GEN_TMP/$GEN_PKG_DIR" >&2
    exit 1
fi
echo "==> Patched $patched generated loader(s) to load the library through ART first"

rm -rf "${JAVA_OUT:?}/$GEN_PKG_DIR"
mkdir -p "$JAVA_OUT/$GEN_PKG_DIR"
cp -R "$GEN_TMP/$GEN_PKG_DIR/." "$JAVA_OUT/$GEN_PKG_DIR/"

echo "==> Done."
find "$JAVA_OUT/$GEN_PKG_DIR" -name '*.kt' | sort
