#!/usr/bin/env bash
#
# Builds the Android native libraries via Nix and drops them where a binding
# generator can read them.
#
#   scripts/nix-build-android-so.sh [dest-dir] [all|arm64-only]
#   ANDROID_TARGETS=arm64-only scripts/nix-build-android-so.sh [dest-dir]
#
#   nix build .#fedimint-sdk-android-jni             (all ABIs, default; cachix-cached)
#   nix build .#fedimint-sdk-android-jni-arm64-v8a   (arm64-only, for local emulator/device dev)
#     -> <dest>/<abi>/{libfedimint_sdk,libc++_shared}.so
#
# This is the `.so`-only half — no bindings of any language. It stands on its
# own because the native library is the input every binding generator shares:
# `generate-android-bindings.sh` reads the metadata straight out of the `.so`
# built here, and any other generator added later reads the same one. It also
# means iterating on the bindings needs no native rebuild.
#
# `dest-dir` defaults to the Gradle project's jniLibs, which is AGP's default
# JNI location, so nothing else has to be configured for the AAR to package it.
#
# Nothing compiles on this machine if the Cachix cache is warm; a cold run
# cross-compiles the crate (heavy: rocksdb + aws-lc from C).
set -euo pipefail

if (( $# > 2 )); then
  echo "usage: $0 [dest-dir] [all|arm64-only]" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-}"
if [[ -z "$DEST" ]]; then
  DEST="$ROOT/android/fedimint-sdk/src/main/jniLibs"
fi
TARGETS="${2:-${ANDROID_TARGETS:-all}}"

case "$TARGETS" in
  all)        attr="fedimint-sdk-android-jni" ;;
  arm64-only) attr="fedimint-sdk-android-jni-arm64-v8a" ;;
  *)
    echo "unknown targets: $TARGETS (expected all | arm64-only)" >&2
    exit 1
    ;;
esac

echo "==> nix build .#$attr"
out="$(nix build "$ROOT#$attr" \
  --accept-flake-config --no-link --print-out-paths)"

echo "==> Syncing $out/jniLibs -> $DEST"
rm -rf "$DEST"
mkdir -p "$DEST"
cp -RL "$out/jniLibs/." "$DEST/"
chmod -R u+w "$DEST"   # Nix store outputs are read-only; make them replaceable

echo "==> Done."
find "$DEST" -type f | sort
