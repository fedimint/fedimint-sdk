# Cross-compiled Android build of `rust/fedimint-sdk`'s `uniffi` feature,
# exposed as cacheable Nix derivations, plus the host tool that turns one into
# language bindings.
#
#   .#fedimint-sdk-android-<triple>        the cross-compiled cdylib
#   .#fedimint-sdk-android-<triple>-deps   crane deps-only build (cache seed)
#   .#fedimint-sdk-android-jni             jniLibs/<abi>/{libfedimint_sdk,libc++_shared}.so
#   .#fedimint-uniffi-bindgen              host build of rust/uniffi-bindgen
#
# Note that nothing depends on `.#fedimint-uniffi-bindgen` today:
# scripts/generate-kotlin-bindings.sh builds that crate with plain cargo, since
# its only dependency is `uniffi` and doing so keeps the whole Kotlin half free
# of Nix. The derivation is also currently broken — crane's vendoring loses
# `uniffi_bindgen`'s askama.toml, so its templates fail to compile.
#
# `-jni` is the native library on its own — no bindings of any language — and
# it is deliberately the last step Nix takes. Generating bindings is neither
# expensive nor a cross-compile: it reads the UniFFI metadata out of the built
# `.so` in seconds. Keeping it outside means the shared, costly half is built
# and cached exactly once (.github/workflows/android-native.yaml) and each
# binding generator is a separate, cheap step over that same artifact —
# scripts/generate-kotlin-bindings.sh being the one that exists today.
#
# Ported from the fedimint-sdk-ffi repo's flake.nix (rev 6873aa3); the crane +
# flakebox cross-compile scaffold is unchanged, retargeted from
# fedimint-client-uniffi to fedimint-sdk.
{
  system,
  nixpkgs,
  flakebox,
  android-nixpkgs,
}:
let
  pkgs = import nixpkgs {
    inherit system;
    config = {
      allowUnfree = true;
      android_sdk.accept_license = true;
    };
  };
  lib = nixpkgs.lib;

  # Used only for `libc++_shared.so` (a target-ABI runtime lib the NDK ships;
  # the exact NDK revision barely matters for it). The cross-compile itself
  # runs through flakebox's own bundled Android SDK — see the note below.
  androidSdk = android-nixpkgs.sdk."${system}" (
    sdkPkgs: with sdkPkgs; [
      cmdline-tools-latest
      build-tools-36-0-0
      platform-tools
      platforms-android-36
      ndk-27-1-12297006
    ]
  );

  flakeboxLib = flakebox.lib.mkLib pkgs {
    config = {
      toolchain.channel = "stable";
      github.ci.enable = false;
      typos.pre-commit.enable = false;
    };
  };

  # NOTE: at the pinned flakebox rev, `mkStdTargets` only uses `androidSdk`'s
  # presence to gate the `android-*` target attrs — the cross-compile env it
  # generates comes from flakebox's own default Android SDK (NDK 25.2, API 24),
  # not the NDK 27.1 the `.#android` dev shell / `scripts/build-android-so.sh`
  # use. The `.so` still runs on API 28+ (forward compatible); the 16 KB
  # page-align link args in `rust/fedimint-sdk/.cargo/config.toml` are applied
  # regardless (lld honours them). Aligning this build to NDK 27.1 is a
  # separate, build-affecting change.
  stdTargets = flakeboxLib.mkStdTargets {
    inherit androidSdk;
  };

  toolchain = flakeboxLib.mkFenixToolchain {
    components = [
      "rustc"
      "cargo"
      "rust-src"
    ];
    targets = lib.getAttrs [
      "default"
      "aarch64-android"
      "x86_64-android"
    ] stdTargets;
  };

  craneLib = toolchain.craneLib;

  # `.cargo/config.toml` carries the Android 16 KB page-align rustflags and
  # `sdallocx_stub.c` is compiled by `build.rs` on Android — both must be in
  # the copied source. `uniffi*.toml` are read by the bindgen step from the
  # crate path directly, not from here.
  src =
    let
      crateDir = ../rust/fedimint-sdk;
      # `.cargo` (the dir) and `config.toml` (the file inside it) so crane does
      # not prune the subtree before reaching the rustflags; `sdallocx_stub.c`
      # for `build.rs`.
      keep = [
        ".cargo"
        "config.toml"
        "sdallocx_stub.c"
      ];
      filter =
        path: type: lib.elem (baseNameOf path) keep || craneLib.filterCargoSources path type;
    in
    lib.cleanSourceWith {
      src = crateDir;
      inherit filter;
      name = "fedimint-sdk-source";
    };

  # Build `rust/fedimint-sdk --features uniffi` for one Android target.
  # Exposes `deps` on its own so CI can push it to Cachix: a source edit then
  # only recompiles the crate, not the whole cross-compiled dependency tree.
  buildOne =
    {
      targetKey,
      rustTarget,
    }:
    let
      target = stdTargets.${targetKey} { };
      commonArgs = target.args // {
        inherit src;
        pname = "fedimint-sdk-android-${rustTarget}";
        version = "0.1.0-alpha.1";
        cargoExtraArgs = "--locked --target ${rustTarget} --lib --features uniffi";
        CARGO_BUILD_TARGET = rustTarget;
        doCheck = false;
        strictDeps = true;
        # rocksdb needs cmake; aws-lc-sys needs cmake + perl + go; python3 is
        # used by some ring / aws-lc generation scripts.
        nativeBuildInputs =
          (target.args.nativeBuildInputs or [ ])
          ++ [
            pkgs.cmake
            pkgs.pkg-config
            pkgs.perl
            pkgs.python3
            pkgs.go
          ];
      };
      deps = craneLib.buildDepsOnly commonArgs;
    in
    {
      inherit deps;
      lib = craneLib.buildPackage (commonArgs // { cargoArtifacts = deps; });
    };

  androidShipped = [
    {
      targetKey = "aarch64-android";
      rustTarget = "aarch64-linux-android";
      abi = "arm64-v8a";
      ndkTriple = "aarch64-linux-android";
    }
    {
      targetKey = "x86_64-android";
      rustTarget = "x86_64-linux-android";
      abi = "x86_64";
      ndkTriple = "x86_64-linux-android";
    }
  ];

  perTarget = lib.listToAttrs (
    map (
      t: lib.nameValuePair t.rustTarget (buildOne { inherit (t) targetKey rustTarget; })
    ) androidShipped
  );

  # The `.so` payload, ABI-laid-out for AGP's default `src/main/jniLibs`. This
  # is the artifact every binding generator reads — nothing Kotlin here.
  #
  # `libc++_shared.so` is the NDK's shared C++ runtime; rocksdb and aws-lc link
  # it, and nothing else puts it in an APK, so without it the app dies at load
  # with `UnsatisfiedLinkError: ... "libc++_shared.so" not found`.
  androidJni = pkgs.runCommand "fedimint-sdk-android-jni" { } ''
    ${lib.concatMapStringsSep "\n" (t: ''
      mkdir -p "$out/jniLibs/${t.abi}"
      cp ${perTarget.${t.rustTarget}.lib}/lib/libfedimint_sdk.so "$out/jniLibs/${t.abi}/"
      libcxx=$(find ${androidSdk} -name libc++_shared.so -path '*/${t.ndkTriple}/*' | head -n1)
      test -n "$libcxx" || { echo "no libc++_shared.so for ${t.ndkTriple} in the NDK" >&2; exit 1; }
      cp "$libcxx" "$out/jniLibs/${t.abi}/"
      chmod u+w "$out/jniLibs/${t.abi}"/*.so
    '') androidShipped}
  '';

  # Host `uniffi-bindgen` — pinned to the same `uniffi` `rust/fedimint-sdk`
  # links, so it reads the metadata baked into the `.so` correctly. A version
  # skew here does not degrade gracefully: the reader walks the metadata with
  # the wrong layout and fails partway through a record, so this pin is what
  # keeps codegen honest rather than merely tidy. Built with
  # crane (no `CARGO_BUILD_TARGET` -> host), which vendors with `cargo` and so
  # handles the `+spec` build-metadata crate versions in its lockfile that
  # nixpkgs' `importCargoLock` on the pinned rev does not. Its only real
  # dependency is `uniffi`, so this is quick.
  uniffiBindgen = craneLib.buildPackage {
    src = craneLib.cleanCargoSource ../rust/uniffi-bindgen;
    pname = "fedimint-uniffi-bindgen";
    version = "0.1.0-alpha.1";
    cargoExtraArgs = "--locked";
    doCheck = false;
    strictDeps = true;
  };
in
{
  fedimint-sdk-android-jni = androidJni;
  fedimint-uniffi-bindgen = uniffiBindgen;
}
// lib.mapAttrs' (t: b: lib.nameValuePair "fedimint-sdk-android-${t}" b.lib) perTarget
// lib.mapAttrs' (t: b: lib.nameValuePair "fedimint-sdk-android-${t}-deps" b.deps) perTarget
