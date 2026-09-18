#!/usr/bin/env bash
#
# Builds the whole Android SDK payload and drops it into the Gradle project:
#
#   -> android/fedimint-sdk/src/main/jniLibs/<abi>/{libfedimint_sdk,libc++_shared}.so
#   -> android/fedimint-sdk/src/main/java/org/fedimint/sdk/*.kt
#
# Both halves are gitignored: they are regenerated, not committed. There is
# no hand-written Kotlin — the generated bindings are the whole API, because
# the crate's `#[uniffi::export]`s hand out fedimint-sdk's real types rather
# than binding-only copies of them (see rust/uniffi-bindgen/DECISION.md).
#
#   scripts/build-android-sdk.sh            nix-build-android-so.sh, then
#                                            generate-kotlin-bindings.sh: the
#                                            same two scripts CI runs as
#                                            android-native.yaml and
#                                            kotlin-sdk.yaml
#   scripts/build-android-sdk.sh --local    cross-compile locally with
#                                            cargo-ndk instead of Nix, for a
#                                            machine that cannot or should not
#                                            use it, then the same
#                                            generate-kotlin-bindings.sh
#
# Only the cargo-ndk build lives here; everything else is delegated, so a
# local build and CI can never run different bindgen invocations.
#
# The --local path needs the NDK, the Android Rust targets, cargo-ndk, and
# the cmake + gnumake + go aws-lc-sys's C sources need on PATH — `nix develop
# .#android` (or CI's non-Nix runner) supplies all of that; fully outside Nix,
# set ANDROID_NDK_HOME (or ANDROID_HOME/ANDROID_SDK_ROOT, from which the
# newest installed NDK is picked) and install cargo-ndk yourself.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE_DIR="$ROOT/rust/fedimint-sdk"
SDK_MAIN="$ROOT/android/fedimint-sdk/src/main"
JNI_LIBS="$SDK_MAIN/jniLibs"
LIB_NAME="fedimint_sdk"
# Keep in sync with android/fedimint-sdk/build.gradle.kts defaultConfig.minSdk
MIN_SDK=28

MODE="nix"
if [[ "${1:-}" == "--local" ]]; then
    MODE="local"
elif [[ "${1:-}" != "" ]]; then
    echo "usage: $0 [--local]" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Native library
# ---------------------------------------------------------------------------

if [[ "$MODE" == "nix" ]]; then
    "$ROOT/scripts/nix-build-android-so.sh" "$JNI_LIBS"
else
    if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
        SDK_ROOT="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
        if [[ -n "$SDK_ROOT" && -d "$SDK_ROOT/ndk" ]]; then
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

    echo "==> Building arm64-v8a + x86_64 .so via cargo-ndk (release)"
    rm -rf "$JNI_LIBS"
    (cd "$CRATE_DIR" && cargo ndk \
        --target arm64-v8a \
        --target x86_64 \
        --platform "$MIN_SDK" \
        --output-dir "$JNI_LIBS" \
        build --release --lib --features uniffi)

    # cargo-ndk copies every cdylib it finds in the target dir, which sweeps up
    # a few dependency cdylibs (iroh's, at the time of writing) alongside ours.
    # They are never loaded — everything is statically linked into our one
    # .so — so packaging them would add megabytes to the AAR for nothing.
    echo "==> Pruning dependency cdylibs cargo-ndk swept into jniLibs"
    find "$JNI_LIBS" -name '*.so' ! -name "lib${LIB_NAME}.so" -print -delete

    # The library links the NDK's *shared* C++ runtime, because rocksdb and
    # aws-lc are C++ and that is what the NDK toolchain links by default.
    # Nothing else puts it in the APK, so without this the app dies at load
    # with `UnsatisfiedLinkError: dlopen failed: library "libc++_shared.so"
    # not found`. Nix's jniLibs output already includes it; the local path
    # has to copy it out of the NDK itself.
    echo "==> Copying libc++_shared.so out of the NDK"
    NDK_LIB_ROOT="$(dirname "$(find "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" \
        -path '*/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so' | head -n1)")/.."
    for abi in arm64-v8a:aarch64-linux-android x86_64:x86_64-linux-android; do
        dest="$JNI_LIBS/${abi%%:*}"
        src="$NDK_LIB_ROOT/${abi##*:}/libc++_shared.so"
        [[ -f "$src" ]] || {
            echo "no libc++_shared.so for ${abi%%:*} at $src" >&2
            exit 1
        }
        cp "$src" "$dest/"
    done
fi

for abi in arm64-v8a x86_64; do
    [[ -f "$JNI_LIBS/$abi/lib${LIB_NAME}.so" ]] \
        || {
            echo "missing $abi/lib${LIB_NAME}.so after build" >&2
            exit 1
        }
done

# ---------------------------------------------------------------------------
# Kotlin bindings
# ---------------------------------------------------------------------------

"$ROOT/scripts/generate-kotlin-bindings.sh" "$JNI_LIBS"
