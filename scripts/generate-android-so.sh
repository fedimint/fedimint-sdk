#!/usr/bin/env bash
#
# The NON-NIX escape hatch: cross-compiles fedimint-sdk's `uniffi` feature
# (.so, arm64-v8a + x86_64) with cargo-ndk and regenerates the UniFFI Kotlin
# bindings from it, into android/fedimint-sdk/src/main/.
#
# The supported path is Nix: `just build-kotlin` runs `nix build
# .#fedimint-sdk-android-jni` (cachix-cached — see nix/ffi.nix) and then
# generates the Kotlin from what it produced, which is also what CI's two
# workflows do. Use this script only when you cannot or do not want to go
# through
# Nix; `just build-android-local` wraps it in the `.#android` shell, which
# supplies the NDK, the android rust toolchains, cargo-ndk, and the cmake +
# gnumake + go that aws-lc-sys builds its C sources with.
#
# Fully outside Nix you need all of the above on PATH plus ANDROID_NDK_HOME
# (or ANDROID_HOME/ANDROID_SDK_ROOT, from which the newest installed NDK is
# picked).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/rust/fedimint-sdk"
SDK_MAIN="$ROOT/android/fedimint-sdk/src/main"
JNI_LIBS="$SDK_MAIN/jniLibs"
JAVA_OUT="$SDK_MAIN/java"
LIB_NAME="fedimint_sdk"
# Keep in sync with android/fedimint-sdk/build.gradle.kts defaultConfig.minSdk
MIN_SDK=28

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    SDK_ROOT="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
    if [ -n "$SDK_ROOT" ] && [ -d "$SDK_ROOT/ndk" ]; then
        ANDROID_NDK_HOME="$(find "$SDK_ROOT/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n1)"
    fi
fi
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an installed NDK (e.g. \$ANDROID_HOME/ndk/<version>)}"
export ANDROID_NDK_HOME
echo "Using ANDROID_NDK_HOME=$ANDROID_NDK_HOME"

command -v cargo-ndk >/dev/null || {
    echo "cargo-ndk not found on PATH. Install with: cargo install cargo-ndk" >&2
    exit 1
}

cd "$CRATE_DIR"

echo "==> Building arm64-v8a + x86_64 .so via cargo-ndk (release)"
rm -rf "$JNI_LIBS"
cargo ndk \
    --target arm64-v8a \
    --target x86_64 \
    --platform "$MIN_SDK" \
    --output-dir "$JNI_LIBS" \
    build --release --lib --features uniffi

# cargo-ndk copies every cdylib it finds in the target dir, which sweeps up a
# few dependency cdylibs (iroh's, at the time of writing) alongside ours. They
# are never loaded — everything is statically linked into our one .so — so
# packaging them would add megabytes to the AAR for nothing.
echo "==> Pruning dependency cdylibs cargo-ndk swept into jniLibs"
find "$JNI_LIBS" -name '*.so' ! -name "lib${LIB_NAME}.so" -print -delete

for abi in arm64-v8a x86_64; do
    [ -f "$JNI_LIBS/$abi/lib${LIB_NAME}.so" ] \
        || { echo "missing $abi/lib${LIB_NAME}.so after build" >&2; exit 1; }
done

# The library links the NDK's *shared* C++ runtime, because rocksdb and
# aws-lc are C++ and that is what the NDK toolchain links by default. Nothing
# else puts it in the APK, so without this the app dies at load with
#   UnsatisfiedLinkError: dlopen failed: library "libc++_shared.so" not found
# Copied after the prune above, which would otherwise delete it.
echo "==> Copying libc++_shared.so out of the NDK"
NDK_LIB_ROOT="$(dirname "$(find "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" \
    -path '*/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so' | head -n1)")/.."
for abi in arm64-v8a:aarch64-linux-android x86_64:x86_64-linux-android; do
    dest="$JNI_LIBS/${abi%%:*}"
    src="$NDK_LIB_ROOT/${abi##*:}/libc++_shared.so"
    [ -f "$src" ] || { echo "no libc++_shared.so for ${abi%%:*} at $src" >&2; exit 1; }
    cp "$src" "$dest/"
    echo "    ${abi%%:*}/libc++_shared.so"
done

echo "==> Regenerating Kotlin bindings from the arm64-v8a library"
MERGED_CONFIG="$(mktemp)"
GEN_TMP="$(mktemp -d)"
trap 'rm -f "$MERGED_CONFIG"; rm -rf "$GEN_TMP"' EXIT

# uniffi.toml supplies this crate's [bindings.kotlin] package_name and
# cdylib_name; uniffi-android.toml supplies [defaults.kotlin] (android=true,
# android_cleaner=true), applied to every component. uniffi-bindgen auto-loads
# only uniffi.toml, so the two are concatenated into one --config here.
cat uniffi.toml uniffi-android.toml >"$MERGED_CONFIG"

# `--library` reads the metadata baked into the built .so, so the bindings can
# never drift from the binary they are generated against.
#
# The generator is its own crate (rust/uniffi-bindgen) whose only dependency is
# uniffi. A bin inside fedimint-sdk would inherit that package's dependencies
# and compile the whole fedimint tree for the host — slow everywhere, and
# broken on a macOS host, which reaches iroh's `netwatch` BSD backend.
cargo run --release \
    --manifest-path "$ROOT/rust/uniffi-bindgen/Cargo.toml" \
    -- generate \
    --library "$JNI_LIBS/arm64-v8a/lib${LIB_NAME}.so" \
    --language kotlin \
    --config "$MERGED_CONFIG" \
    --out-dir "$GEN_TMP"

# uniffi-bindgen nests its output by package path, so uniffi.toml's
# package_name of org.fedimint.sdk lands in org/fedimint/sdk/. There is no
# hand-written Kotlin: the generated bindings are the API, because the exports
# hand out fedimint-sdk's real types rather than binding-only copies of them.
# The directory is therefore 100% generated and is wiped wholesale.
GEN_PKG_DIR="org/fedimint/sdk"
rm -rf "${JAVA_OUT:?}/$GEN_PKG_DIR"
mkdir -p "$JAVA_OUT/$GEN_PKG_DIR"
if [ -d "$GEN_TMP/$GEN_PKG_DIR" ]; then
    cp -R "$GEN_TMP/$GEN_PKG_DIR/." "$JAVA_OUT/$GEN_PKG_DIR/"
else
    echo "uniffi-bindgen wrote nothing to $GEN_PKG_DIR — check uniffi.toml's package_name" >&2
    find "$GEN_TMP" -name '*.kt' >&2
    exit 1
fi

echo "==> Done."
echo "jniLibs:"
find "$JNI_LIBS" -type f
echo "Kotlin bindings:"
find "$JAVA_OUT/$GEN_PKG_DIR" -name "*.kt" | sort
