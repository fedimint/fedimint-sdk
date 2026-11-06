#!/usr/bin/env bash
#
# Builds the Android native libraries via Nix and drops them where a binding
# generator can read them.
#
#   scripts/nix-build-android-so.sh [dest-dir]
#
#   nix build .#fedimint-sdk-android-jni   (cachix-cached; see nix/ffi.nix)
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

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-}"
if [[ -z "$DEST" ]]; then
  DEST="$ROOT/android/fedimint-sdk/src/main/jniLibs"
fi
TARGETS="${2:-all}"

if [[ "$TARGETS" == "arm64-only" ]]; then
  echo "==> nix build .#fedimint-sdk-android-aarch64-linux-android (arm64-only)"
  out="$(nix build "$ROOT#fedimint-sdk-android-aarch64-linux-android" \
    --accept-flake-config --no-link --print-out-paths)"
  mkdir -p "$DEST/arm64-v8a"
  cp "$out/lib/libfedimint_sdk.so" "$DEST/arm64-v8a/"
  # If the full jniLibs is already cached or reachable, sync libc++_shared.so
  jni_out="$(nix build "$ROOT#fedimint-sdk-android-jni" --accept-flake-config --no-link --print-out-paths 2>/dev/null || true)"
  if [[ -n "$jni_out" && -f "$jni_out/jniLibs/arm64-v8a/libc++_shared.so" ]]; then
    cp "$jni_out/jniLibs/arm64-v8a/libc++_shared.so" "$DEST/arm64-v8a/"
  fi
  chmod -R u+w "$DEST/arm64-v8a"
else
  echo "==> nix build .#fedimint-sdk-android-jni"
  out="$(nix build "$ROOT#fedimint-sdk-android-jni" \
    --accept-flake-config --no-link --print-out-paths)"

  echo "==> Syncing $out/jniLibs -> $DEST"
  rm -rf "$DEST"
  mkdir -p "$DEST"
  cp -RL "$out/jniLibs/." "$DEST/"
  chmod -R u+w "$DEST"   # Nix store outputs are read-only; make them replaceable
fi

echo "==> Done."
find "$DEST" -type f | sort
