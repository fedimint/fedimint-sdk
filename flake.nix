{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    fedimint = {
      # Devimint input - Should point to a release tag, as it doesn't need to be updated often.
      url = "github:fedimint/fedimint/v0.10.0";
    };
    fedimint-wasm = {
         url = "github:fedimint/fedimint/v0.10.0";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "fedimint/nixpkgs";
    };
  };
  outputs =
    {
      self,
      flake-utils,
      fedimint,
      fedimint-wasm,
      fenix,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        nixpkgs = fedimint.inputs.nixpkgs;
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
             (import "${fedimint}/nix/overlays/esplora-electrs.nix")
          ];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };
        androidSdk = pkgs.androidenv.composeAndroidPackages {
          includeNDK = true;
          toolsVersion = "26.1.1";
          ndkVersions = ["27.1.12297006"];
          includeSystemImages = true;
          buildToolsVersions = ["36.0.0"];
          platformVersions = ["36"];
          abiVersions = ["arm64-v8a" "x86_64"];
          cmdLineToolsVersion = "13.0";
        };
        
        # Xcode wrapper to expose system tools in the impure Nix shell
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

        fenixPkgs = fenix.packages.${system};
        baseToolchain = fenixPkgs.stable.toolchain;
        
        mkToolchain = targets: fenixPkgs.combine (
          [ baseToolchain ]
          ++ (map (t: fenixPkgs.targets.${t}.stable.rust-std) targets)
        );

        androidToolchain = mkToolchain [
          "aarch64-linux-android"
          "armv7-linux-androideabi"
          "i686-linux-android"
          "x86_64-linux-android"
        ];
        
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
          ];

          commonShellHook = ''
            export PLAYWRIGHT_BROWSERS_PATH=${pkgs.playwright-driver.browsers}
            export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
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
            pkgs.playwright-driver.browsers
          ];

          wasmShellHook = ''
            export PLAYWRIGHT_BROWSERS_PATH=${pkgs.playwright-driver.browsers}
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
            export BINDGEN_EXTRA_CLANG_ARGS_armv7_linux_androideabi="--sysroot=$TOOLCHAIN/sysroot -I$TOOLCHAIN/lib/clang/$CLANG_VER/include -I$TOOLCHAIN/lib64/clang/$CLANG_VER/include"
            export BINDGEN_EXTRA_CLANG_ARGS_i686_linux_android="--sysroot=$TOOLCHAIN/sysroot -I$TOOLCHAIN/lib/clang/$CLANG_VER/include -I$TOOLCHAIN/lib64/clang/$CLANG_VER/include"

            # Force bindgen to use NDK clang instead of any Homebrew/system LLVM
            # This prevents aws-lc-sys build failures when Homebrew LLVM is installed
            if [ -f "$TOOLCHAIN/bin/clang" ]; then
              export CLANG_PATH="$TOOLCHAIN/bin/clang"
            fi
            
          '';

          iosShellHook = ''
            export PATH=${xcode-wrapper}/bin:$PATH

            if [[ "$OSTYPE" == "darwin"* ]]; then
                unset SDKROOT
                unset NIX_CFLAGS_COMPILE
                unset NIX_LDFLAGS

                # Unset generic compiler variables to avoid Nix wrapper
                unset CC CXX LD AR NM RANLIB
                
                # Force usage of system tools found in PATH (via xcode-wrapper)
                export AR=/usr/bin/ar
                export CC=clang
                export CXX=clang++
                
                # Explicitly set compilers for targets to system clang
                export CC_aarch64_apple_ios=clang
                export CC_x86_64_apple_ios=clang
                export CC_aarch64_apple_darwin=clang
                export CC_x86_64_apple_darwin=clang
                
                export CXX_aarch64_apple_ios=clang++
                export CXX_x86_64_apple_ios=clang++
                export CXX_aarch64_apple_darwin=clang++
                export CXX_x86_64_apple_darwin=clang++

                unset CC_aarch64_apple_ios_sim
                unset CC_x86_64_apple_ios_sim
                unset LD_aarch64_apple_ios LD_aarch64_apple_darwin LD_aarch64_apple_ios_sim
                unset LD_x86_64_apple_ios LD_x86_64_apple_ios_sim LD_x86_64_apple_darwin
                
                # Unset Nix include paths to prevent interference with system SDK
                unset CPATH
                unset C_INCLUDE_PATH
                unset CPLUS_INCLUDE_PATH
                unset OBJC_INCLUDE_PATH

                unset CPATH
                unset C_INCLUDE_PATH
                unset CPLUS_INCLUDE_PATH
                unset OBJC_INCLUDE_PATH

                # Remove Nix libiconv from path to rely on system SDK
                # export LIBRARY_PATH=${pkgs.libiconv}/lib:$LIBRARY_PATH
                
                # Force usage of system Xcode
                export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
                
                # Do NOT set SDKROOT globally; let xcrun/rustc find the correct one (iphoneos vs iphonesimulator)
                unset SDKROOT
                
                export SNAPPY_STATIC=1

                # Set deployment targets
                export MACOSX_DEPLOYMENT_TARGET="15.0"
                export IPHONEOS_DEPLOYMENT_TARGET="15.0"

                # Force bindgen to use Xcode clang instead of any Homebrew/system LLVM
                # This prevents aws-lc-sys build failures when Homebrew LLVM is installed
                export CLANG_PATH=$(xcrun --find clang 2>/dev/null || which clang)

                # Set BINDGEN_EXTRA_CLANG_ARGS for iOS cross-compilation targets
                IOS_SDKROOT=$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null || true)
                SIM_SDKROOT=$(xcrun --sdk iphonesimulator --show-sdk-path 2>/dev/null || true)
                if [ -n "$IOS_SDKROOT" ]; then
                  export BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_ios="--sysroot=$IOS_SDKROOT"
                fi
                if [ -n "$SIM_SDKROOT" ]; then
                  # x86_64 and aarch64-sim need the simulator SDK (iPhoneOS SDK is ARM-only)
                  export BINDGEN_EXTRA_CLANG_ARGS_x86_64_apple_ios="--sysroot=$SIM_SDKROOT"
                  # aws-lc-sys bundles an older bindgen that passes "aarch64-apple-ios-sim" to clang,
                  # but clang expects "aarch64-apple-ios-simulator". Override the target explicitly.
                  # See: https://github.com/rust-lang/rust-bindgen/pull/3182
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

          ios = pkgs.mkShellNoCC {
            # Set as derivation env var so it can't be overridden by user shell profiles
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            nativeBuildInputs = commonNativeBuildInputs ++ [
               pkgs.cmake
               pkgs.go
               pkgs.libclang # Needed for bindgen (aws-lc-sys etc.)
               iosToolchain
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
        packages = {
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
