# Cross-compiled Android (.so) and iOS (.a) builds of the in-tree
# fedimint-client-uniffi crate, exposed as cacheable Nix derivations.
#
# Ported from the fedimint-sdk-ffi repo's flake.nix (rev 6873aa3) when that
# repo was merged in here; the build logic is unchanged apart from taking its
# inputs as arguments and reading the crate source from this repo.
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
  # lib from the flake output and isDarwin from the system string, so that
  # computing the returned attr *names* (which every `nix build .#wasmBundle`
  # does through the merge in flake.nix) never forces the pkgs import above.
  lib = nixpkgs.lib;
  isDarwin = lib.hasSuffix "-darwin" system;

  # NOTE: at the pinned flakebox rev, mkStdTargets only uses this argument's
  # presence to gate the android-* target attrs — the cross-compile env it
  # generates comes from flakebox's own default Android SDK (NDK 25.2, API
  # level 24), not from this NDK 27.1 composition. Inherited as-is from the
  # fedimint-sdk-ffi repo; switching the builds to this SDK is a separate,
  # build-affecting change.
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

  # `mkStdTargets` provides target descriptors (mkIOSTarget for ios-*,
  # mkAndroidTarget for android-*, etc.) that wire up the right
  # CC/AR/LINKER/RUSTFLAGS env vars per cargo target triple.
  # Each entry is a lambda; calling it with `{}` materialises
  # `{ args, componentTargets }`.
  stdTargets = flakeboxLib.mkStdTargets {
    inherit androidSdk;
  };

  # Fenix toolchain combining all the cross-compile std libraries we
  # need (host + android + ios on darwin).
  toolchain = flakeboxLib.mkFenixToolchain {
    components = [
      "rustc"
      "cargo"
      "rust-src"
    ];
    targets = lib.getAttrs (
      [
        "default"
        "aarch64-android"
        "x86_64-android"
      ]
      ++ lib.optionals isDarwin [
        "aarch64-ios"
        "aarch64-ios-sim"
        "x86_64-ios"
      ]
    ) stdTargets;
  };

  craneLib = toolchain.craneLib;

  # Keep sdallocx_stub.c (compiled by build.rs) alongside the Rust sources
  # craneLib.filterCargoSources keeps. The uniffi*.toml configs are
  # deliberately NOT included: these builds only run `cargo build --lib`
  # (bindgen runs later via ubrn, outside Nix), so including bindgen-only
  # config would needlessly invalidate every cross-compile on edits to it.
  src =
    let
      crateDir = ../fedimint-client-uniffi;
      filter =
        path: type: baseNameOf path == "sdallocx_stub.c" || craneLib.filterCargoSources path type;
    in
    lib.cleanSourceWith {
      src = crateDir;
      inherit filter;
      name = "source";
    };

  # Symlink-only derivation that exposes /usr/bin/* and the
  # Xcode.app dirs to the Nix build sandbox. Same pattern fedi uses
  # for their `nix develop .#xcode` shell. `__noChroot = true` so
  # the symlink targets are accessible at build time; this requires
  # the Nix daemon to allow relaxed sandboxing
  # (`sandbox = relaxed` in nix.conf or `--option sandbox relaxed`).
  xcode-wrapper = pkgs.runCommand "xcode-wrapper-impure" { __noChroot = true; } ''
    mkdir -p $out/bin
    ln -s /usr/bin/ld $out/bin/ld
    ln -s /usr/bin/clang $out/bin/clang
    ln -s /usr/bin/clang++ $out/bin/clang++
    ln -s /usr/bin/cc $out/bin/cc
    ln -s /usr/bin/c++ $out/bin/c++
    ln -s /usr/bin/ar $out/bin/ar
    ln -s /usr/bin/xcrun $out/bin/xcrun
    ln -s /usr/bin/xcode-select $out/bin/xcode-select
    ln -s /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild $out/bin/xcodebuild
  '';

  # Build the crate for a single (rustTarget, targetKey) pair, returning
  # `{ deps, lib }`: the crane dependency-only derivation and the final
  # library build on top of it. Exposing deps as its own package lets CI
  # push it to Cachix, so a source edit only recompiles the crate itself
  # instead of the whole cross-compiled dependency tree.
  # `targetKey` is the flakebox-stdTargets key (e.g. `aarch64-ios`),
  # `rustTarget` is the Cargo triple (e.g. `aarch64-apple-ios`).
  buildOne =
    {
      targetKey,
      rustTarget,
      isIos ? false,
    }:
    let
      target = stdTargets.${targetKey} { };
      commonArgs =
        target.args
        // (lib.optionalAttrs isDarwin {
          # nixpkgs' stdenv walks `buildInputs` and adds each `/lib` to
          # the cc-wrapper's NIX_LDFLAGS. Putting libiconv here is what
          # makes `cc -liconv` resolve in the host build-script link
          # step on macOS 14+ (where iconv lives only in the Apple SDK).
          # iOS cross-compile linker invocations also see this path but
          # harmlessly skip the wrong-arch Mach-O lib (with a warning)
          # and resolve via the SDK paths supplied by mkIOSTarget.
          buildInputs = [ pkgs.libiconv ];
        })
        // (lib.optionalAttrs isIos {
          # iOS cross-compile reads /Applications/Xcode.app and /usr/bin
          # via the xcode-wrapper symlinks; this requires relaxed
          # sandboxing.
          __noChroot = true;
          IPHONEOS_DEPLOYMENT_TARGET = "15.0";
          MACOSX_DEPLOYMENT_TARGET = "14.0";

          # nixpkgs' darwin stdenv sets SDKROOT to its bundled
          # apple-sdk-11 (a macOS SDK) and points DEVELOPER_DIR into
          # the Nix store. When cc-rs's build script runs
          # `xcrun --sdk iphoneos --show-sdk-path` to find the
          # iPhoneOS SDK, those Nix-store paths confuse xcrun and it
          # exits 255. Reset to the real /Applications/Xcode.app so
          # xcrun resolves SDKs via xcode-select.
          #
          # Mirrors the iosShellHook in flake.nix + fedi's xcode dev shell.
          preBuild = ''
            unset SDKROOT
            unset NIX_CFLAGS_COMPILE
            unset NIX_LDFLAGS
            # APPEND (not prepend) /usr/bin so xcrun resolves but
            # bare `tar` still picks up Nix's GNU tar — crane's deps
            # archive uses GNU-only `--sort=name` and would fail
            # against macOS's BSD tar.
            export PATH=$PATH:/usr/bin:/Applications/Xcode.app/Contents/Developer/usr/bin
            export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer

            # Nix's cc-wrapper hardcodes --sysroot to a Nix-store
            # SDK that lacks libSystem on modern macOS runners.
            # Bypass it for host builds by pointing Cargo's host
            # linker to the system clang, which resolves libSystem
            # natively via xcrun.
            export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/cc
            export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/usr/bin/cc
          '';
        })
        // {
          inherit src;
          pname = "fedimint-client-uniffi-${rustTarget}";
          version = "0.1.0";
          cargoExtraArgs = "--locked --target ${rustTarget} --lib";
          CARGO_BUILD_TARGET = rustTarget;
          doCheck = false;
          strictDeps = true;
          # rocksdb needs cmake; aws-lc-sys needs cmake + perl + go.
          # python3 is needed by some ring/aws-lc generation scripts.
          # gnutar overrides macOS's BSD tar so crane's depsArchive
          # (`tar --sort=name`) works.
          nativeBuildInputs =
            (target.args.nativeBuildInputs or [ ])
            ++ [
              pkgs.gnutar
              pkgs.cmake
              pkgs.pkg-config
              pkgs.perl
              pkgs.python3
              pkgs.go
            ]
            ++ lib.optionals isIos [ xcode-wrapper ];
      };
      deps = craneLib.buildDepsOnly commonArgs;
    in
    {
      inherit deps;
      lib = craneLib.buildPackage (commonArgs // { cargoArtifacts = deps; });
    };

  ##############
  # Android
  ##############

  # Targets we actually ship .so files for (matches ubrn.config.yaml
  # in packages/react-native-bindings). The toolchain has more wired
  # up so adding more is one entry per row below.
  androidShipped = [
    {
      targetKey = "aarch64-android";
      rustTarget = "aarch64-linux-android";
      abi = "arm64-v8a";
    }
    {
      targetKey = "x86_64-android";
      rustTarget = "x86_64-linux-android";
      abi = "x86_64";
    }
  ];

  androidPerTargetBuilds = lib.listToAttrs (
    map (
      t:
      lib.nameValuePair t.rustTarget (buildOne {
        inherit (t) targetKey rustTarget;
      })
    ) androidShipped
  );

  androidJniLibs = pkgs.runCommand "fedimint-uniffi-android-jniLibs" { } ''
    mkdir -p $out/jniLibs
    ${lib.concatMapStringsSep "\n" (t: ''
      mkdir -p $out/jniLibs/${t.abi}
      cp ${androidPerTargetBuilds.${t.rustTarget}.lib}/lib/libfedimint_client_uniffi.so \
         $out/jniLibs/${t.abi}/
    '') androidShipped}
  '';

  ##############
  # iOS (darwin only)
  ##############

  iosShipped = [
    {
      targetKey = "aarch64-ios";
      rustTarget = "aarch64-apple-ios";
    }
    {
      targetKey = "aarch64-ios-sim";
      rustTarget = "aarch64-apple-ios-sim";
    }
    {
      targetKey = "x86_64-ios";
      rustTarget = "x86_64-apple-ios";
    }
  ];

  iosPerTargetBuilds = lib.listToAttrs (
    map (
      t:
      lib.nameValuePair t.rustTarget (buildOne {
        inherit (t) targetKey rustTarget;
        isIos = true;
      })
    ) iosShipped
  );

  # Layout matches what `xcodebuild -create-xcframework` consumes:
  #   ios-arm64/                      device slice (aarch64-apple-ios)
  #   ios-arm64_x86_64-simulator/    fat sim slice (lipo'd)
  # The xcframework wrap stays in ubrn so the uniffi-generated
  # module map and headers can be folded in there.
  iosBundle =
    pkgs.runCommand "fedimint-uniffi-ios-libs"
      {
        __noChroot = true;
      }
      ''
        export PATH=/usr/bin:$PATH
        mkdir -p $out/ios-arm64
        cp ${iosPerTargetBuilds."aarch64-apple-ios".lib}/lib/libfedimint_client_uniffi.a \
           $out/ios-arm64/

        mkdir -p $out/ios-arm64_x86_64-simulator
        /usr/bin/lipo -create \
          ${iosPerTargetBuilds."aarch64-apple-ios-sim".lib}/lib/libfedimint_client_uniffi.a \
          ${iosPerTargetBuilds."x86_64-apple-ios".lib}/lib/libfedimint_client_uniffi.a \
          -output $out/ios-arm64_x86_64-simulator/libfedimint_client_uniffi.a
      '';
in
{
  androidBundle = androidJniLibs;
}
// lib.mapAttrs' (t: b: lib.nameValuePair "android-${t}" b.lib) androidPerTargetBuilds
// lib.mapAttrs' (t: b: lib.nameValuePair "android-${t}-deps" b.deps) androidPerTargetBuilds
// lib.optionalAttrs isDarwin (
  {
    iosBundle = iosBundle;
  }
  // lib.mapAttrs' (t: b: lib.nameValuePair "ios-${t}" b.lib) iosPerTargetBuilds
  // lib.mapAttrs' (t: b: lib.nameValuePair "ios-${t}-deps" b.deps) iosPerTargetBuilds
)
