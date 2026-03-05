{
  description = "acme-distributor - ACME certificate distributor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Common build inputs
        buildInputs = with pkgs; [
          openssl
        ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.darwin.apple_sdk.frameworks.Security
          pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          rustToolchain
        ];

        # Server package
        acme-distributor = pkgs.rustPlatform.buildRustPackage {
          pname = "acme-distributor";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit buildInputs nativeBuildInputs;

          # Only build the server binary
          cargoBuildFlags = [ "-p" "acme-distributor-server" ];

          meta = with pkgs.lib; {
            description = "ACME certificate distributor server";
            license = licenses.mit;
          };
        };

        # Client package
        acme-distributor-client = pkgs.rustPlatform.buildRustPackage {
          pname = "acme-distributor-client";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit buildInputs nativeBuildInputs;

          # Only build the client binary
          cargoBuildFlags = [ "-p" "acme-distributor-client" ];

          meta = with pkgs.lib; {
            description = "ACME certificate distributor client";
            license = licenses.mit;
          };
        };

      in {
        packages = {
          inherit acme-distributor acme-distributor-client;
          default = acme-distributor;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = buildInputs ++ [
            rustToolchain
            pkgs.pkg-config
            pkgs.diesel-cli
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      }
    ) // {
      # NixOS modules
      nixosModules = {
        acme-distributor = import ./module.nix;
        acme-shim = import ./module-client.nix;
      };

      # Overlay for use in other flakes
      overlays.default = final: prev: {
        acme-distributor = self.packages.${prev.system}.acme-distributor;
        acme-distributor-client = self.packages.${prev.system}.acme-distributor-client;
      };
    };
}
