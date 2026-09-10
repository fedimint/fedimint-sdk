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
# `generate-kotlin-bindings.sh` reads the metadata straight out of the `.so`
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
DEST="${1:-$ROOT/android/fedimint-sdk/src/main/jniLibs}"

echo "==> nix build .#fedimint-sdk-android-jni"
out="$(nix build "$ROOT#fedimint-sdk-android-jni" \
  --accept-flake-config --no-link --print-out-paths)"

echo "==> Syncing $out/jniLibs -> $DEST"
rm -rf "$DEST"
mkdir -p "$DEST"
cp -RL "$out/jniLibs/." "$DEST/"
chmod -R u+w "$DEST"   # Nix store outputs are read-only; make them replaceable

echo "==> Done."
find "$DEST" -type f | sort
