# Cross-compiled Android and wasm builds of `rust/fedimint-sdk`'s `uniffi` feature,
# exposed as cacheable Nix derivations, plus the host tool that turns one into
# language bindings.
#
#   .#fedimint-sdk-android-<triple>        the cross-compiled cdylib
#   .#fedimint-sdk-android-<triple>-deps   crane deps-only build (cache seed)
#   .#fedimint-sdk-android-jni             jniLibs/<abi>/{libfedimint_sdk,libc++_shared}.so
#   .#fedimint-sdk-wasm                    the wasm32-unknown-unknown module (lib/fedimint_sdk.wasm)
#   .#fedimint-sdk-wasm-deps               crane deps-only build (cache seed)
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
  # not the NDK 27.1 the `.#android` dev shell / `scripts/build-android-sdk.sh
  # --local` use. The `.so` still runs on API 28+ (forward compatible); the 16 KB
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
    # The iOS target attrs only exist on Darwin (flakebox's mkStdTargets gates
    # them on `isDarwin`), so they are added conditionally. Android/Wasm CI runs
    # on Linux, where this list is unchanged — so adding iOS cannot invalidate the
    # cached Android/Wasm toolchain on the runner that builds it.
    targets = lib.getAttrs (
      [
        "default"
        "aarch64-android"
        "x86_64-android"
        "wasm32-unknown"
      ]
      ++ lib.optionals pkgs.stdenv.isDarwin [
        "aarch64-ios"
        "aarch64-ios-sim"
        "x86_64-ios"
      ]
    ) stdTargets;
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

  # Build `rust/fedimint-sdk --features uniffi` for one target (an Android ABI or wasm32).
  # Exposes `deps` on its own so CI can push it to Cachix: a source edit then
  # only recompiles the crate, not the whole cross-compiled dependency tree.
  buildOne =
    {
      pname,
      targetKey,
      rustTarget,
    }:
    let
      # `extraRustFlags` has no default on `wasm32-unknown`'s target function (unlike the
      # Android ones), so it is always supplied here rather than only where it is needed.
      target = stdTargets.${targetKey} { extraRustFlags = ""; };
      commonArgs = target.args // lib.optionalAttrs pkgs.stdenv.isDarwin {
        # nixpkgs' stdenv walks `buildInputs` and adds each `/lib` to the
        # cc-wrapper's NIX_LDFLAGS. Putting libiconv here is what makes
        # `cc -liconv` resolve in the host build-script link step on macOS
        # 14+ (where iconv lives only in the Apple SDK).
        buildInputs = [ pkgs.libiconv ];
      } // {
        inherit src pname;
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

  # ---------------------------------------------------------------------------
  # iOS / macOS
  # ---------------------------------------------------------------------------
  #
  # Apple targets build against an SDK that ships inside Xcode and cannot live
  # in the nix store — but that only rules out a *pure* derivation, not a
  # cacheable one. `__noChroot = true` (honoured when the builder runs with
  # `sandbox = relaxed`) lets these reach /usr/bin and /Applications/Xcode.app,
  # and the result is an ordinary store path that Cachix serves like any other.
  # Ported from fedimint-sdk-ffi's flake.nix, which has been doing this against
  # the same dependency graph (aws-lc-sys, librocksdb-sys, iroh).
  #
  # Known soundness gap, accepted here as upstream accepts it: the host Xcode is
  # read but is *not* an input to the derivation hash, so a cache hit can hand
  # back an archive built against a different Xcode than the consumer has. For a
  # static archive of Rust code that is nearly always benign; it is not
  # guaranteed to be. Bump a target's `pname` if you ever need to force a
  # rebuild across an Xcode upgrade.

  # flakebox's mkIOSTarget points CC/LD/linker at /usr/bin/clang and /usr/bin/cc
  # but sets no `__noChroot` of its own, so without this the tools it names are
  # simply not there. xcodebuild comes from the Xcode bundle; everything else is
  # the /usr/bin shim, which resolves through xcode-select.
  xcodeWrapper = pkgs.runCommand "xcode-wrapper-impure" { __noChroot = true; } ''
    mkdir -p $out/bin
    for tool in ld clang clang++ cc c++ ar lipo xcrun xcode-select; do
      ln -s /usr/bin/$tool $out/bin/$tool
    done
    ln -s /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild $out/bin/xcodebuild
  '';

  # Build `rust/fedimint-sdk --features uniffi` for one Apple target. Same shape
  # as `buildOne` above, plus the three things an impure Apple build needs.
  buildOneApple =
    {
      targetKey,
      rustTarget,
    }:
    let
      target = stdTargets.${targetKey} { };
      commonArgs = target.args // {
        inherit src;
        pname = "fedimint-sdk-ios-${rustTarget}";
        version = "0.1.0-alpha.1";
        cargoExtraArgs = "--locked --target ${rustTarget} --lib --features uniffi";
        CARGO_BUILD_TARGET = rustTarget;
        doCheck = false;
        strictDeps = true;

        # On both this and the deps-only derivation below, because the deps pass
        # is where rocksdb and aws-lc actually compile — it needs host access
        # just as much as the crate pass does.
        __noChroot = true;

        # Must agree with ios/Package.swift and scripts/build-ios-lib.sh, or the
        # C objects are built for a different minimum than the Swift linking them.
        IPHONEOS_DEPLOYMENT_TARGET = "15.0";
        MACOSX_DEPLOYMENT_TARGET = "13.0";

        # nixpkgs' Darwin stdenv points SDKROOT and NIX_CFLAGS_COMPILE at its own
        # bundled SDK, which breaks `xcrun --sdk iphoneos --show-sdk-path` and
        # sends the C builds at the wrong headers. Clearing them restores plain
        # Apple clang behaviour.
        preBuild = ''
          unset SDKROOT
          unset NIX_CFLAGS_COMPILE
          unset NIX_LDFLAGS
          # Appended, never prepended: Nix's GNU tar has to keep priority over
          # BSD tar, because crane's depsArchive depends on `--sort=name`.
          export PATH=$PATH:/usr/bin:/Applications/Xcode.app/Contents/Developer/usr/bin
          export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
          # Build scripts and proc macros are compiled for the *host* even on a
          # cross-compile, and they have to link with Apple's cc for the same
          # reason everything else here does.
          export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/cc
          export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/usr/bin/cc
        '';

        nativeBuildInputs = [ xcodeWrapper ] ++ (target.args.nativeBuildInputs or [ ]) ++ [
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

  # The four slices ios/ ships. `aarch64-apple-darwin` is not a mistake: the
  # XCFramework carries a macOS slice so `swift test` runs on the host without
  # booting a simulator. Keep in step with DEFAULT_TARGETS in
  # scripts/build-ios-lib.sh.
  appleShipped = [
    { targetKey = "aarch64-ios"; rustTarget = "aarch64-apple-ios"; }
    { targetKey = "aarch64-ios-sim"; rustTarget = "aarch64-apple-ios-sim"; }
    { targetKey = "x86_64-ios"; rustTarget = "x86_64-apple-ios"; }
    # The macOS slice is a *native* build, so it uses the plain native target
    # rather than flakebox's `aarch64-darwin`: that key is gated on
    # `buildPlatform.config == "aarch64-apple-darwin"` while this nixpkgs
    # reports `arm64-apple-darwin`, so it never exists — and it is a pkgsCross
    # clang target anyway, which is not what a host build wants.
    { targetKey = "default"; rustTarget = "aarch64-apple-darwin"; }
  ];

  perAppleTarget = lib.listToAttrs (
    map (t: lib.nameValuePair t.rustTarget (buildOneApple { inherit (t) targetKey rustTarget; })) appleShipped
  );

  # Laid out per *triple*, deliberately not pre-lipo'd into XCFramework slices.
  # scripts/nix-build-ios-lib.sh has to reproduce exactly what the plain-cargo
  # build produces — including `apple-slices.txt`, which records what a run
  # actually built and is what stops a subset build from shipping stale
  # archives. A fat simulator slice cannot be mapped back to a triple, so the
  # lipo stays on the consuming side.
  appleBundle = pkgs.runCommand "fedimint-sdk-ios-bundle" { } ''
    ${lib.concatMapStringsSep "\n" (t: ''
      mkdir -p "$out/lib/${t.rustTarget}"
      cp ${perAppleTarget.${t.rustTarget}.lib}/lib/libfedimint_sdk.a "$out/lib/${t.rustTarget}/"
    '') appleShipped}
  '';

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
      t:
      lib.nameValuePair t.rustTarget (
        buildOne {
          pname = "fedimint-sdk-android-${t.rustTarget}";
          inherit (t) targetKey rustTarget;
        }
      )
    ) androidShipped
  );

  # `rust/fedimint-sdk --features uniffi` for the browser: the module the web binding
  # (js/web/sdk-web) is generated from and ships. Same shape as the Android targets above:
  # the costly build is cached here, and reading the bindings out of it is a separate, cheap
  # step (scripts/generate-sdk-web-bindings.sh). The crate's release profile already sets
  # `opt-level = "z"`, `lto` and `panic = "abort"`.
  wasm = buildOne {
    pname = "fedimint-sdk-wasm";
    targetKey = "wasm32-unknown";
    rustTarget = "wasm32-unknown-unknown";
  };

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
  fedimint-sdk-wasm = wasm.lib;
  fedimint-sdk-wasm-deps = wasm.deps;
}
// lib.mapAttrs' (t: b: lib.nameValuePair "fedimint-sdk-android-${t}" b.lib) perTarget
// lib.mapAttrs' (t: b: lib.nameValuePair "fedimint-sdk-android-${t}-deps" b.deps) perTarget
// lib.optionalAttrs pkgs.stdenv.isDarwin (
  {
    fedimint-sdk-ios-bundle = appleBundle;
  }
  // lib.mapAttrs' (t: b: lib.nameValuePair "fedimint-sdk-ios-${t}" b.lib) perAppleTarget
  // lib.mapAttrs' (t: b: lib.nameValuePair "fedimint-sdk-ios-${t}-deps" b.deps) perAppleTarget
)
