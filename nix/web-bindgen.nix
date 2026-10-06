# The two host tools the web binding needs, pinned so the generated TypeScript and the module
# it drives never drift from each other.
#
#   ubrn              uniffi-bindgen-react-native, built from rust/ubrn, whose Cargo.toml and
#                     Cargo.lock pin the tool's version. Its `wasm2` flavor generates the
#                     TypeScript in js/web/sdk-web/src/generated from the built `.wasm`, and the
#                     React Native bindings come from the same build.
#   wasm-bindgen-cli  exactly the `wasm-bindgen` crate version the SDK's dependency tree links
#                     (`=0.2.106` in rust/fedimint-sdk/Cargo.toml); ubrn shells out to it to
#                     rewrite the module's wasm-bindgen imports into the `_bg.js` glue.
{ pkgs }:
let
  lib = pkgs.lib;
in
{
  ubrn =
    let
      cargoToml = lib.importTOML ../rust/ubrn/Cargo.toml;
    in
    pkgs.rustPlatform.buildRustPackage {
      pname = cargoToml.package.name;
      version = cargoToml.package.version;
      src = ../rust/ubrn;
      cargoLock = {
        lockFile = ../rust/ubrn/Cargo.lock;
        # The tool is a git dependency. This fetches it at the commit Cargo.lock records, so
        # moving the pin is a `cargo update` in rust/ubrn, with no hash to update here.
        allowBuiltinFetchGit = true;
      };
      doCheck = false;
      meta.mainProgram = "ubrn";
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
