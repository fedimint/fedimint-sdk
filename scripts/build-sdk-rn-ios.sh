#!/usr/bin/env bash
#
# Builds rust/fedimint-sdk as a static library for each iOS target, then has ubrn assemble the
# xcframework from those libraries and regenerate js/react-native/react-native-bindings. macOS
# with Xcode only; run inside the `.#ios` shell (`just build-rn-ios`), which provides the iOS
# Rust targets and the nix-packaged `ubrn`.
#
#   UBRN_IOS_TARGETS=aarch64-apple-ios scripts/build-sdk-rn-ios.sh
#
# UBRN_IOS_TARGETS (comma separated) narrows the slices to build; unset, ubrn.config.yaml's three
# (device, arm64 simulator, x86_64 simulator) are built. CI passes the device slice alone on pull
# requests, which roughly halves the cross-compile time.
#
# The libraries are built here and not by ubrn, which is run with `--no-cargo` to read them: the
# manifest lists no `staticlib` (rust/fedimint-sdk/Cargo.toml says why), and ubrn cannot be told to
# ask cargo for one. `--no-cargo` makes ubrn use the libraries it finds at
# <target dir>/<triple>/release/libfedimint_sdk.a, which is where cargo leaves them.
#
# See scripts/generate-sdk-rn-bindings.sh for why a `ubrn` from node_modules is refused.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG="$ROOT/js/react-native/react-native-bindings"
MANIFEST="$ROOT/rust/fedimint-sdk/Cargo.toml"

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

# Match the Swift build's minimum iOS version for both Rust and native dependencies.
# Without this, rustc defaults to iOS 10 while C/C++ builds can use the SDK's version.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"

# The targets to build: those of UBRN_IOS_TARGETS, else the three that ubrn.config.yaml lists
# under `ios.targets`. ubrn is given the same list with `--targets` below, so it assembles
# exactly the libraries built here and never one that another build left in the target
# directory.
triples=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
if [[ -n "${UBRN_IOS_TARGETS:-}" ]]; then
  IFS=, read -r -a triples <<<"$UBRN_IOS_TARGETS"
fi
((${#triples[@]})) || { echo "UBRN_IOS_TARGETS names no target" >&2; exit 1; }
for triple in "${triples[@]}"; do
  [[ "$triple" =~ ^[a-z0-9_-]+$ ]] || {
    echo "UBRN_IOS_TARGETS is '${UBRN_IOS_TARGETS:-}': it must be target triples separated by" \
      "commas, without spaces" >&2
    exit 1
  }
done

# Where cargo puts what it builds for this manifest, which is where ubrn looks for it.
target_dir="$(cd "$ROOT/rust" && cargo metadata --manifest-path "$MANIFEST" --format-version 1 \
  --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
[[ -n "$target_dir" ]] || { echo "cargo metadata named no target directory" >&2; exit 1; }

# Run from rust/, the directory ubrn.config.yaml names as the crate's.
for triple in "${triples[@]}"; do
  echo "==> cargo rustc --locked --lib --release --features uniffi --target $triple" \
    "--crate-type lib --crate-type staticlib"
  (cd "$ROOT/rust" && cargo rustc \
    --manifest-path "$MANIFEST" \
    --locked \
    --lib \
    --release \
    --features uniffi \
    --target "$triple" \
    --crate-type lib \
    --crate-type staticlib)

  # Without the library, ubrn would start a build of its own, which cannot produce one.
  [[ -f "$target_dir/$triple/release/libfedimint_sdk.a" ]] || {
    echo "no libfedimint_sdk.a at $target_dir/$triple/release after building $triple" >&2
    exit 1
  }
done

cd "$PKG"
echo "==> ubrn build ios --no-cargo: assembling the xcframework and generating $PKG"
ubrn build ios --config ubrn.config.yaml --release --no-cargo --and-generate \
  --targets "$(IFS=,; echo "${triples[*]}")"

# ubrn's podspec template declares no system frameworks; the Rust library needs three. Skip
# the patch when a kept podspec already carries them: `patch --forward` on an applied hunk exits 1.
if ! grep -q '^  s.frameworks = ' ReactNativeBindings.podspec; then
  patch -p0 --forward < patches/add_ios_frameworks.patch
fi

echo "==> Done."
ls -la "$PKG/FedimintReactNativeBindingsFramework.xcframework"
