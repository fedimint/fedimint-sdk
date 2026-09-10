#!/usr/bin/env bash
#
# Builds the full Android SDK payload and drops it into the Gradle project.
#
#   -> android/fedimint-sdk/src/main/jniLibs/<abi>/*.so
#   -> android/fedimint-sdk/src/main/java/org/fedimint/sdk/*.kt
#
# Both halves are gitignored: they are regenerated, not committed.
#
# This is a convenience wrapper over the two steps CI runs as separate
# workflows, in the same order and with the same scripts:
#
#   1. nix-build-android-so.sh      the cross-compiled native library
#                                   (.github/workflows/android-native.yaml)
#   2. generate-kotlin-bindings.sh  the Kotlin read out of that library
#                                   (.github/workflows/kotlin-sdk.yaml)
#
# There is deliberately no third path that does both at once: the split is the
# point. The native library is expensive and shared, the bindings are cheap and
# per-language, and `uniffi-bindgen` reading the built `.so` rather than the
# crate source is what stops the two from drifting apart.
#
# Nothing compiles on this machine if the Cachix cache is warm; a cold run
# cross-compiles the crate (heavy: rocksdb + aws-lc from C).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT/scripts/nix-build-android-so.sh"
"$ROOT/scripts/generate-kotlin-bindings.sh"
