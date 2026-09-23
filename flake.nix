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
        # The SDK the Gradle builds and `cargo ndk` need. The emulator and its system image
        # add gigabytes to the closure, so they live in a second composition used only by the
        # `.#android-emulator` shell (scripts/android-emulator.sh).
        mkAndroidSdk = extra: pkgs.androidenv.composeAndroidPackages ({
          includeNDK = true;
          toolsVersion = "26.1.1";
          ndkVersions = ["27.1.12297006"];
          # The CMake the Android Gradle plugin asks for when a module does not pin one (React
          # Native's app template); the SDK directory is read-only, so it cannot fetch it itself.
          cmakeVersions = ["3.22.1"];
          # 35.0.0 is what the Android Gradle plugin picks for a library module that names no
          # version, the React Native bindings package among them; 36.0.0 is what the apps name.
          buildToolsVersions = ["35.0.0" "36.0.0"];
          platformVersions = ["36"];
          cmdLineToolsVersion = "13.0";
        } // extra);
        androidSdk = mkAndroidSdk { };
        # One x86_64 Google APIs image for the platform above; the React Native bindings ship
        # arm64-v8a and x86_64, so this is the emulator ABI they run on.
        androidEmulatorSdk = mkAndroidSdk {
          includeEmulator = true;
          includeSystemImages = true;
          systemImageTypes = ["google_apis"];
          abiVersions = ["x86_64"];
        };

        # Xcode wrapper to expose system tools in the impure Nix shell.
        xcode-wrapper = pkgs.stdenv.mkDerivation {
          name = "xcode-wrapper-impure";
          # Fails in sandbox. Use `--option sandbox relaxed` or `--option sandbox false`.
          __noChroot = true;
          buildCommand = ''
            mkdir -p $out/bin
            ln -s /usr/bin/ld $out/bin/ld
            ln -s /usr/bin/clang $out/bin/clang
            ln -s /usr/bin/clang++ $out/bin/clang++
            # ln -s /usr/bin/xcodebuild $out/bin/xcodebuild
            ln -s /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild $out/bin/xcodebuild
            ln -s /usr/bin/xcrun $out/bin/xcrun
            ln -s /usr/bin/xcode-select $out/bin/xcode-select
            ln -s /usr/bin/security $out/bin/security
            ln -s /usr/bin/codesign $out/bin/codesign
          '';
        };

        # The wasm2 binding generator and the wasm-bindgen version it shells out to. See
        # nix/web-bindgen.nix.
        webBindgen = import ./nix/web-bindgen.nix { inherit pkgs; };

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

        # The three slices `ubrn build ios` assembles into the xcframework: a device build plus
        # both simulator architectures.
        iosToolchain = mkToolchain [
          "aarch64-apple-ios"
          "aarch64-apple-ios-sim"
          "x86_64-apple-ios"
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
            # wasm-opt, the last step of scripts/generate-sdk-web-bindings.sh.
            pkgs.binaryen
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
            pkgs.cmake
            pkgs.rustPlatform.bindgenHook
            playwrightBrowsers
            webBindgen.ubrn
            webBindgen.wasm-bindgen-cli
          ];

          wasmShellHook = ''
            export PLAYWRIGHT_BROWSERS_PATH=${playwrightBrowsers}
            export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
            export LIBCLANG_PATH="${pkgs.libclang.lib}/lib"
            export LD_LIBRARY_PATH="${pkgs.stdenv.cc.cc.lib}/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            
            # bindgenHook exports a global BINDGEN_EXTRA_CLANG_ARGS which breaks
            # cross-compilation (e.g., wasm32). We capture it and scope it strictly
            # to the host target so native builds like aws-lc-sys succeed.
            if [ -n "''${BINDGEN_EXTRA_CLANG_ARGS:-}" ]; then
              HOST_TARGET=$(rustc -vV | sed -n 's|host: ||p' | tr '-' '_')
              export "BINDGEN_EXTRA_CLANG_ARGS_''${HOST_TARGET}=$BINDGEN_EXTRA_CLANG_ARGS"
              unset BINDGEN_EXTRA_CLANG_ARGS
            fi
          '';

          mkAndroidShellHook = sdk: ''
            export ANDROID_HOME=${sdk.androidsdk}/libexec/android-sdk
            export ANDROID_SDK_ROOT=$ANDROID_HOME
            export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk-bundle
            export ANDROID_NDK_HOME=$ANDROID_NDK_ROOT
            export NDK_HOME=$ANDROID_NDK_ROOT
            export ROCKSDB_STATIC=1

            # pkgs.libclang (needed for bindgen) puts its unwrapped clang/clang++
            # ahead of the properly wrapped host compiler on $PATH, so host-target
            # builds (e.g. `cargo check`, or any build.rs compiled for the native
            # target rather than an Android target) pick a clang with no macOS SDK
            # header search paths and fail with "'cstdint' file not found". Pin
            # CC/CXX to the wrapped compiler explicitly; cargo-ndk sets its own
            # per-target CC_*/CXX_* when actually cross-compiling, so this only
            # affects host-target builds.
            export CC="${pkgs.stdenv.cc}/bin/cc"
            export CXX="${pkgs.stdenv.cc}/bin/c++"

            # fedimint-core/fedimint-connectors enable aws-lc-sys's "bindgen"
            # feature, which makes aws-lc-sys skip its pregenerated bindings and
            # fall back to a full CMake source build. That CMakeLists.txt
            # unconditionally references the `tool/`, `tool-openssl/` directories
            # and `util/go_tests.txt`, none of which the published crates.io
            # tarball actually ships, so the CMake configure step fails. Force
            # aws-lc-sys's alternate cc-only builder instead, which compiles the
            # same sources directly via the `cc` crate and never touches those
            # missing paths; bindgen itself still runs fine off of libclang.
            export AWS_LC_SYS_CMAKE_BUILDER=0

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

          mkAndroidShell = sdk: pkgs.mkShell {
            # Set as derivation env var so it can't be overridden by user shell profiles
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            nativeBuildInputs = commonNativeBuildInputs ++ [
              sdk.androidsdk
              # The JDK Gradle runs on, the version CI's kotlin-sdk job installs too.
              pkgs.jdk17
              # scripts/rn-example.sh asks Metro whether it is up.
              pkgs.curl
              pkgs.cmake
              pkgs.gnumake
              pkgs.go
              pkgs.cargo-ndk
              pkgs.libclang # Often needed for bindgen
              androidToolchain
              # The binding generator scripts/generate-sdk-rn-bindings.sh runs over the built .so.
              webBindgen.ubrn
            ];
            shellHook = commonShellHook + mkAndroidShellHook sdk;
          };
          iosShellHook = ''
            export PATH=${xcode-wrapper}/bin:$PATH

            if [[ "$OSTYPE" == "darwin"* ]]; then
                unset SDKROOT
                unset NIX_CFLAGS_COMPILE
                unset NIX_LDFLAGS

                # Unset generic compiler variables to avoid Nix wrapper.
                unset CC CXX LD AR NM RANLIB

                # Force usage of system tools found in PATH (via xcode-wrapper).
                export AR=/usr/bin/ar
                export CC=clang
                export CXX=clang++

                # Explicitly set compilers for targets to system clang.
                export CC_aarch64_apple_ios=clang
                export CC_x86_64_apple_ios=clang
                export CC_aarch64_apple_darwin=clang
                export CC_x86_64_apple_darwin=clang

                export CXX_aarch64_apple_ios=clang++
                export CXX_x86_64_apple_ios=clang++
                export CXX_aarch64_apple_darwin=clang++
                export CXX_x86_64_apple_darwin=clang++

                # Bypass Nix's cc-wrapper for host builds: it hardcodes --sysroot to an
                # incompatible apple-sdk-11 store path that lacks libSystem.dylib on modern
                # macOS runners, causing "symbol not found" errors for _writev, _sysconf, etc.
                export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/cc
                export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=/usr/bin/cc

                unset CC_aarch64_apple_ios_sim
                unset CC_x86_64_apple_ios_sim
                unset LD_aarch64_apple_ios LD_aarch64_apple_darwin LD_aarch64_apple_ios_sim
                unset LD_x86_64_apple_ios LD_x86_64_apple_ios_sim LD_x86_64_apple_darwin

                # Unset Nix include paths to prevent interference with system SDK.
                unset CPATH
                unset C_INCLUDE_PATH
                unset CPLUS_INCLUDE_PATH
                unset OBJC_INCLUDE_PATH

                # Force usage of system Xcode.
                export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer

                # Do NOT set SDKROOT globally; let xcrun/rustc find the correct one (iphoneos
                # vs iphonesimulator).
                unset SDKROOT

                export SNAPPY_STATIC=1

                # Set deployment targets.
                export MACOSX_DEPLOYMENT_TARGET="15.0"
                export IPHONEOS_DEPLOYMENT_TARGET="15.0"

                # Force bindgen to use Xcode clang instead of any Homebrew/system LLVM. This
                # prevents aws-lc-sys build failures when Homebrew LLVM is installed.
                export CLANG_PATH=$(xcrun --find clang 2>/dev/null || which clang)

                # Set BINDGEN_EXTRA_CLANG_ARGS for iOS cross-compilation targets.
                IOS_SDKROOT=$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null || true)
                SIM_SDKROOT=$(xcrun --sdk iphonesimulator --show-sdk-path 2>/dev/null || true)
                if [ -n "$IOS_SDKROOT" ]; then
                  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--sysroot=$IOS_SDKROOT"
                fi
                if [ -n "$SIM_SDKROOT" ]; then
                  # x86_64 and aarch64-sim need the simulator SDK (iPhoneOS SDK is ARM-only).
                  export BINDGEN_EXTRA_CLANG_ARGS_x86_64_apple_ios="--sysroot=$SIM_SDKROOT"
                  # aws-lc-sys bundles an older bindgen that passes "aarch64-apple-ios-sim" to
                  # clang, but clang expects "aarch64-apple-ios-simulator". Override the target
                  # explicitly. See https://github.com/rust-lang/rust-bindgen/pull/3182.
                  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios_sim="--sysroot=$SIM_SDKROOT --target=arm64-apple-ios-simulator"
                fi

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

          android = mkAndroidShell androidSdk;
          # The same shell plus the emulator and one system image, for running the React Native
          # example apps: `just android-emulator` boots the device, `just rn-example` installs an
          # app on it. Both run in this one shell so a single `adb` talks to the device; two adb
          # builds on one machine keep restarting each other's server and leave it "offline".
          android-emulator = mkAndroidShell androidEmulatorSdk;

          # macOS only. Cargo cross-compiles rust/fedimint-sdk for the three iOS slices with
          # Xcode's toolchain; ubrn assembles the xcframework and regenerates the bindings
          # (just build-rn-ios).
          ios = pkgs.mkShellNoCC {
            # Set as derivation env var so it can't be overridden by user shell profiles
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            nativeBuildInputs = commonNativeBuildInputs ++ [
               pkgs.cmake
               pkgs.go
               pkgs.libclang # Needed for bindgen (aws-lc-sys etc.)
               iosToolchain
               webBindgen.ubrn
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
               xcode-wrapper
            ];
            shellHook = commonShellHook + iosShellHook;
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
          # `.so` / `-deps` derivations. Also `fedimint-sdk-wasm` (the wasm32
          # module the web binding is generated from) and its `-deps` build.
          # See nix/ffi.nix.
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
            # The wasm2 binding generator and the wasm-bindgen version it shells out to.
            # See nix/web-bindgen.nix.
            ubrn = webBindgen.ubrn;
            wasm-bindgen-cli = webBindgen.wasm-bindgen-cli;
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
