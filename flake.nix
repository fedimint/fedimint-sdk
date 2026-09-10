{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    fedimint = {
      # Devimint input. This is no longer a "point it at a release tag when
      # convenient" pin: rust/fedimint-sdk compiles against fedimint master and
      # repeats this revision in its Cargo.toml, so the client under test and
      # the federation it is tested against come from one commit. Move this
      # input, fedimint-wasm below, and rust/fedimint-sdk/Cargo.toml together —
      # the pins-agree job in .github/workflows/rust-sdk-ci.yaml fails the build
      # when they drift.
      url = "github:fedimint/fedimint?rev=1ab15e4b89aaa35727ca16e401d0cbc7d066de58";
    };
    fedimint-wasm = {
      # The wasm client is built from this revision; keep it in sync with the
      # devimint input above and with rust/fedimint-sdk/Cargo.toml, so that
      # every client in this repo and the federation they are tested against
      # come from the same commit.
      url = "github:fedimint/fedimint?rev=1ab15e4b89aaa35727ca16e401d0cbc7d066de58";
    };
    nixpkgs-playwright = {
      # Playwright browsers have to match the `playwright` version pnpm-lock.yaml
      # resolves (1.56.1), which is why they get their own pin instead of coming
      # from whichever nixpkgs the fedimint input happens to carry: bumping that
      # input moved the browsers to 1.59.1 and the tests could not find them.
      url = "github:NixOS/nixpkgs/76701a179d3a98b07653e2b0409847499b2a07d3";
    };
    # nixpkgs, fenix, flakebox and android-nixpkgs feed the Android cross-compile
    # derivations in nix/ffi.nix (`.#fedimint-sdk-android*`). Pinned to the same
    # revisions the fedimint-sdk-ffi repo's flake.lock used before it was merged
    # in here, since that combination is known to build.
    nixpkgs = {
      # nixos-25.05
      url = "github:NixOS/nixpkgs/ac62194c3917d5f474c1a844b6fd6da2db95077d";
    };
    fenix = {
      # Pinned like the inputs below: fenix moves nightly, and every bump
      # invalidates all toolchain and cross-compile derivations.
      url = "github:nix-community/fenix/298b12d701ef0d12c0f2e4858d4208bee24d14e5";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flakebox = {
      url = "github:rustshop/flakebox/fa493d2de9db942e4d03934e6599d756a70388d4";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
    };
    android-nixpkgs = {
      # stable channel
      url = "github:tadfisher/android-nixpkgs/a2b56f05390f7ad84c158eefd1877fbd9e4d2825";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      flake-utils,
      fedimint,
      fedimint-wasm,
      nixpkgs,
      nixpkgs-playwright,
      fenix,
      flakebox,
      android-nixpkgs,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import fedimint.inputs.nixpkgs {
          inherit system;
          overlays = [
             (import "${fedimint}/nix/overlays/esplora-electrs.nix")
          ];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        # Kept separate from `pkgs` on purpose: these have to match the
        # `playwright` version in pnpm-lock.yaml, not the fedimint bump.
        playwrightBrowsers =
          (import nixpkgs-playwright { inherit system; }).playwright-driver.browsers;
        # No emulator system images: nothing in this repo runs an emulator, and
        # each ABI's image adds gigabytes to the dev shell closure.
        androidSdk = pkgs.androidenv.composeAndroidPackages {
          includeNDK = true;
          toolsVersion = "26.1.1";
          ndkVersions = ["27.1.12297006"];
          buildToolsVersions = ["36.0.0"];
          platformVersions = ["36"];
          cmdLineToolsVersion = "13.0";
        };

        fenixPkgs = fenix.packages.${system};
        baseToolchain = fenixPkgs.stable.toolchain;
        
        mkToolchain = targets: fenixPkgs.combine (
          [ baseToolchain ]
          ++ (map (t: fenixPkgs.targets.${t}.stable.rust-std) targets)
        );

        # The two ABIs the Android SDK ships. The cacheable cross-compile lives
        # in nix/ffi.nix (its own flakebox toolchain); this one backs the
        # `.#android` dev shell for Gradle work and manual `cargo ndk` runs.
        androidToolchain = mkToolchain [
          "aarch64-linux-android"
          "x86_64-linux-android"
        ];

        wasmToolchain = mkToolchain [
          "wasm32-unknown-unknown"
        ];
        
        defaultToolchain = mkToolchain [];
      in
      {
        devShells = let
          # Minimal dependencies for CI steps that just run pnpm commands
          commonNativeBuildInputs = [
            pkgs.pnpm
            pkgs.nodejs_24
            pkgs.git
            pkgs.gh
            pkgs.zip
            pkgs.coreutils
            pkgs.patch
            pkgs.just
          ];

          # Used by the android shell; referencing playwright-driver here would
          # pull the browser bundles (>1 GiB) into its closure, so only the wasm
          # shells (via wasmShellHook) set up Playwright.
          commonShellHook = ''
            export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
          '';

          # Dependencies that were previously common, likely for general dev/testing/wasm
          wasmNativeBuildInputs = commonNativeBuildInputs ++ [
            pkgs.bitcoind
            pkgs.electrs
            pkgs.jq
            pkgs.lnd
            pkgs.netcat
            pkgs.perl
            pkgs.esplora-electrs
            pkgs.procps
            pkgs.which
            pkgs.go
            pkgs.libclang
            playwrightBrowsers
          ];

          wasmShellHook = ''
            export PLAYWRIGHT_BROWSERS_PATH=${playwrightBrowsers}
            export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
            export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
          '';

          androidShellHook = ''
            export ANDROID_HOME=${androidSdk.androidsdk}/libexec/android-sdk
            export ANDROID_SDK_ROOT=$ANDROID_HOME
            export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk-bundle
            export ANDROID_NDK_HOME=$ANDROID_NDK_ROOT
            export NDK_HOME=$ANDROID_NDK_ROOT
            export ROCKSDB_STATIC=1
            
            # Dynamically determine host tag (darwin-x86_64 or linux-x86_64)
            NDK_PREBUILT=$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt
            HOST_TAG=$(ls $NDK_PREBUILT | head -n 1)
            TOOLCHAIN=$NDK_PREBUILT/$HOST_TAG
            
            # Find clang version for headers
            CLANG_VER=$(ls $TOOLCHAIN/lib/clang/ | head -n 1)
            if [ -z "$CLANG_VER" ]; then
                CLANG_VER=$(ls $TOOLCHAIN/lib64/clang/ | head -n 1) 
            fi
            
            export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$TOOLCHAIN/sysroot -I$TOOLCHAIN/lib/clang/$CLANG_VER/include -I$TOOLCHAIN/lib64/clang/$CLANG_VER/include"
            export BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android="--sysroot=$TOOLCHAIN/sysroot -I$TOOLCHAIN/lib/clang/$CLANG_VER/include -I$TOOLCHAIN/lib64/clang/$CLANG_VER/include"

            # Force bindgen to use NDK clang instead of any Homebrew/system LLVM
            # This prevents aws-lc-sys build failures when Homebrew LLVM is installed
            if [ -f "$TOOLCHAIN/bin/clang" ]; then
              export CLANG_PATH="$TOOLCHAIN/bin/clang"
            fi

          '';
        in {
          default = pkgs.mkShell {
            nativeBuildInputs = commonNativeBuildInputs;
          };

          wasm = pkgs.mkShell {
             nativeBuildInputs = wasmNativeBuildInputs ++ [ wasmToolchain ];
             shellHook = wasmShellHook;
          };

          android = pkgs.mkShell {
            # Set as derivation env var so it can't be overridden by user shell profiles
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            nativeBuildInputs = commonNativeBuildInputs ++ [
              androidSdk.androidsdk
              pkgs.cmake
              pkgs.gnumake
              pkgs.go
              pkgs.cargo-ndk
              pkgs.libclang # Often needed for bindgen
              androidToolchain
            ];
            shellHook = commonShellHook + androidShellHook;
          };

          wasm-tests = pkgs.mkShell {
             nativeBuildInputs = wasmNativeBuildInputs ++ [
               fedimint.packages.${system}.devimint
               fedimint.packages.${system}.gateway-pkgs
               fedimint.packages.${system}.fedimint-pkgs
               fedimint.packages.${system}.fedimint-recurringd
               fedimint.packages.${system}.fedimint-recurringdv2
             ] ++ [ wasmToolchain ];
             shellHook = wasmShellHook;
          };
        };
        packages =
          # Cacheable cross-compiled builds of `rust/fedimint-sdk`'s `uniffi`
          # feature for Android, plus the Kotlin bindings generated from them:
          # `fedimint-sdk-android` (jniLibs + Kotlin), `fedimint-sdk-android-jni`
          # (jniLibs only — the React-Native-reusable half), and per-target
          # `.so` / `-deps` derivations. See nix/ffi.nix.
          import ./nix/ffi.nix {
            inherit
              system
              nixpkgs
              flakebox
              android-nixpkgs
              ;
          }
          // {
            wasmBundle = fedimint-wasm.packages.${system}.wasmBundle;
          };
      }
    );
  nixConfig = {
    extra-substituters = [ 
      "https://fedimint.cachix.org"
      "https://fedibtc.cachix.org"
      "https://nix-community.cachix.org"
    ];
    extra-trusted-public-keys = [
      "fedimint.cachix.org-1:FpJJjy1iPVlvyv4OMiN5y9+/arFLPcnZhZVVCHCDYTs="
      "fedibtc.cachix.org-1:KyG8I1663EYQm2ThciPUvjm1r9PHiZbOYz4goj+U76k="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
    ];
  };
}
