#!/usr/bin/env bash
#
# Generates js/react-native/react-native-bindings from already built Android libraries of
# rust/fedimint-sdk, and stages those libraries where the Gradle project expects them.
#
#   scripts/generate-sdk-rn-bindings.sh [<store path of .#fedimint-sdk-android-jni>]
#     -> js/react-native/react-native-bindings/src/generated/{fedimint_sdk.ts,fedimint_sdk-ffi.ts}
#        js/react-native/react-native-bindings/cpp/generated/{fedimint_sdk.cpp,fedimint_sdk.hpp}
#        the turbo-module files ubrn owns (android/CMakeLists.txt, android/cpp-adapter.cpp, ...)
#        js/react-native/react-native-bindings/android/src/main/jniLibs/<abi>/libfedimint_sdk.so
#
# The libraries default to the nix build (`.#fedimint-sdk-android-jni`, nix/ffi.nix), which is
# what CI reads and what the committed bindings must match. ubrn reads the UniFFI metadata out
# of the `.so` itself, so the TypeScript and C++ can never describe a library other than the one
# the app loads.
#
# ubrn finds the libraries through `cargo metadata`'s target directory, so the nix output is laid
# out under a throwaway CARGO_TARGET_DIR and ubrn is told not to build (`--no-cargo`). Run inside
# the `.#android` shell: it needs the nix-packaged `ubrn` and `cargo` on PATH, and
# `pnpm install` done in js/ so ubrn formats its TypeScript with the workspace's prettier.
#
# The `uniffi-bindgen-react-native` npm package the bindings depend on (for its C++ runtime
# headers and CocoaPod) ships its own `ubrn` at UniFFI 0.31, which would write bindings that do
# not match the crate. That is why this script refuses a `ubrn` from node_modules and why the
# package has no `ubrn:*` scripts: `pnpm run` puts node_modules/.bin first on PATH.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/js/react-native/react-native-bindings"

for tool in ubrn cargo; do
  command -v "$tool" >/dev/null ||
    { echo "$tool not on PATH; run in the .#android shell" >&2; exit 1; }
done
case "$(command -v ubrn)" in
  */node_modules/*)
    echo "ubrn resolves to the npm package's CLI ($(command -v ubrn)), which generates for" \
      "UniFFI 0.31; run this script from the .#android shell, not through pnpm run" >&2
    exit 1
    ;;
esac

JNI="${1:-$(nix build "$ROOT#fedimint-sdk-android-jni" --accept-flake-config --no-link \
  --print-out-paths)}"
if [[ ! -d "$JNI/jniLibs" ]]; then
  echo "no jniLibs under $JNI" >&2
  echo "build them first: nix build .#fedimint-sdk-android-jni (or pass its store path)" >&2
  exit 1
fi

# ubrn looks for <target dir>/<triple>/release/libfedimint_sdk.so per configured ABI.
TARGET="$(mktemp -d)"
trap 'rm -rf "$TARGET"' EXIT
for pair in arm64-v8a:aarch64-linux-android x86_64:x86_64-linux-android; do
  abi="${pair%%:*}"
  triple="${pair##*:}"
  mkdir -p "$TARGET/$triple/release"
  cp "$JNI/jniLibs/$abi/libfedimint_sdk.so" "$TARGET/$triple/release/"
  chmod u+w "$TARGET/$triple/release/libfedimint_sdk.so"
done

echo "==> Generating $PKG from $JNI"
rm -rf "$PKG/src/generated" "$PKG/cpp/generated"
cd "$PKG"
CARGO_TARGET_DIR="$TARGET" \
  ubrn build android --config ubrn.config.yaml --release --no-cargo --and-generate

# `build android --and-generate` renders only the crossplatform and Android turbo-module
# templates; the podspec is an iOS-only template and is left alone (an existing one is kept
# as is, a missing one stays missing). `generate turbo-module` renders the same crossplatform
# and Android templates again (byte-identical, so this is a no-op for them) plus the iOS-only
# ones, the podspec included; it needs no native library and no iOS toolchain, so it is safe to
# run here. The namespace is the crate name UniFFI derives its default namespace from.
ubrn generate turbo-module --config ubrn.config.yaml fedimint_sdk

# ubrn's podspec template declares no system frameworks; the Rust library needs two. Skip the
# patch when a kept podspec already carries them: `patch --forward` on an applied hunk exits 1.
if ! grep -q '^  s.frameworks = ' ReactNativeBindings.podspec; then
  patch -p0 --forward < patches/add_ios_frameworks.patch
fi

echo "==> Done."
ls -la "$PKG/src/generated" "$PKG/cpp/generated" "$PKG/android/src/main/jniLibs"/*
