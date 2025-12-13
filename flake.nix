{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    fedimint = {
      # Devimint input - Should point to a release tag, as it doesn't need to be updated often.
      url = "github:fedimint/fedimint/v0.8.1";
    };
    fedimint-wasm = {
         url = "github:fedimint/fedimint?rev=c2350bb7528fff6100968d23ddeb626653c5ebb0";
    };
  };
  outputs =
    {
      self,
      flake-utils,
      fedimint,
      fedimint-wasm,
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
        };
      in
      {
        devShells = {
          default = pkgs.mkShell {
            nativeBuildInputs = [
              fedimint.packages.${system}.devimint
              fedimint.packages.${system}.gateway-pkgs
              fedimint.packages.${system}.fedimint-pkgs
              fedimint.packages.${system}.fedimint-recurringd
              pkgs.bitcoind
              pkgs.electrs
              pkgs.jq
              pkgs.lnd
              pkgs.netcat
              pkgs.perl
              pkgs.esplora-electrs
              pkgs.procps
              pkgs.which
              pkgs.git

              pkgs.pnpm
              pkgs.nodejs_20
              # The version of playwright in nixpkgs has to match the version specified in package.json
              pkgs.playwright-driver.browsers
            ];

            shellHook = ''
              export PLAYWRIGHT_BROWSERS_PATH=${pkgs.playwright-driver.browsers}
              export PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
            '';
          };
        };
        packages = {
          wasmBundle = fedimint-wasm.packages.${system}.wasmBundle;
        };
      }
    );
  nixConfig = {
    extra-substituters = [ "https://fedimint.cachix.org" ];
    extra-trusted-public-keys = [
      "fedimint.cachix.org-1:FpJJjy1iPVlvyv4OMiN5y9+/arFLPcnZhZVVCHCDYTs="
    ];
  };
}
