#!/usr/bin/env bash
#
# Cross-compiles rust/fedimint-sdk for iOS, assembles the xcframework and regenerates
# js/react-native/react-native-bindings from the result. macOS with Xcode only; run inside the
# `.#ios` shell (`just build-rn-ios`), which provides the iOS Rust targets and the nix-packaged
# `ubrn`.
#
#   UBRN_IOS_TARGETS=aarch64-apple-ios scripts/build-sdk-rn-ios.sh
#
# UBRN_IOS_TARGETS (comma separated) narrows the slices to build; unset, ubrn.config.yaml's three
# (device, arm64 simulator, x86_64 simulator) are built. CI passes the device slice alone on pull
# requests, which roughly halves the cross-compile time.
#
# See scripts/generate-sdk-rn-bindings.sh for why a `ubrn` from node_modules is refused.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/js/react-native/react-native-bindings"

for tool in ubrn cargo xcodebuild; do
  command -v "$tool" >/dev/null ||
    { echo "$tool not on PATH; run in the .#ios shell on macOS" >&2; exit 1; }
done
case "$(command -v ubrn)" in
  */node_modules/*)
    echo "ubrn resolves to the npm package's CLI ($(command -v ubrn)), which generates for" \
      "UniFFI 0.31; run this script from the nix shell, not through pnpm run" >&2
    exit 1
    ;;
esac

targets=()
if [[ -n "${UBRN_IOS_TARGETS:-}" ]]; then
  targets=(--targets "$UBRN_IOS_TARGETS")
fi

cd "$PKG"
echo "==> Building rust/fedimint-sdk for iOS and generating $PKG"
ubrn build ios --config ubrn.config.yaml --release --and-generate "${targets[@]}"

# ubrn's podspec template declares no system frameworks; the Rust library needs three. Skip
# the patch when a kept podspec already carries them: `patch --forward` on an applied hunk exits 1.
if ! grep -q '^  s.frameworks = ' ReactNativeBindings.podspec; then
  patch -p0 --forward < patches/add_ios_frameworks.patch
fi

echo "==> Done."
ls -la "$PKG/FedimintReactNativeBindingsFramework.xcframework"
