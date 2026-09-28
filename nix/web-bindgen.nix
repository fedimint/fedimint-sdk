# The two host tools the web binding needs, pinned so the generated TypeScript and the module
# it drives never drift from each other.
#
#   ubrn              uniffi-bindgen-react-native, from the fork branch that reads UniFFI 0.32
#                     metadata (jhugman/uniffi-bindgen-react-native#468); rust/fedimint-sdk
#                     links uniffi 0.32.0 and the released tool stops at 0.31. Its `wasm2` flavor
#                     generates the TypeScript in js/web/sdk-web/src/generated from the built
#                     `.wasm`. Move `rev` (and nix/ubrn-Cargo.lock) together when the branch moves.
#   wasm-bindgen-cli  exactly the `wasm-bindgen` crate version the SDK's dependency tree links
#                     (`=0.2.106` in rust/fedimint-sdk/Cargo.toml); ubrn shells out to it to
#                     rewrite the module's wasm-bindgen imports into the `_bg.js` glue.
{ pkgs }:
let
  lib = pkgs.lib;
in
{
  ubrn = pkgs.rustPlatform.buildRustPackage {
    pname = "uniffi-bindgen-react-native";
    version = "0.31.0-5-uniffi-0.32";
    src = pkgs.fetchFromGitHub {
      owner = "Dzejkop";
      repo = "uniffi-bindgen-react-native";
      rev = "06f0a30d257573ffcc852e84e814670d53085979";
      hash = "sha256-Y1QMxMtwzBEVzlo7GohvecTYAMZsMQTqH99cagnzbzQ=";
    };
    cargoLock.lockFile = ./ubrn-Cargo.lock;
    buildAndTestSubdir = "crates/ubrn_cli";
    doCheck = false;
    # `ubrn` is the name the tool's own docs and npm launcher use.
    postInstall = ''
      ln -s $out/bin/uniffi-bindgen-react-native $out/bin/ubrn
    '';
    meta.mainProgram = "uniffi-bindgen-react-native";
  };

  wasm-bindgen-cli = pkgs.rustPlatform.buildRustPackage rec {
    pname = "wasm-bindgen-cli";
    version = "0.2.106";
    src = pkgs.fetchCrate {
      inherit pname version;
      hash = "sha256-M6WuGl7EruNopHZbqBpucu4RWz44/MSdv6f0zkYw+44=";
    };
    cargoHash = "sha256-ElDatyOwdKwHg3bNH/1pcxKI7LXkhsotlDPQjiLHBwA=";
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.openssl ] ++ lib.optionals pkgs.stdenv.isDarwin [ pkgs.curl ];
    # The tests expect the wasm-bindgen monorepo around them.
    doCheck = false;
  };
}
