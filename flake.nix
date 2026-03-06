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
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Use callPackage for the packages
        acme-sh-patched = pkgs.callPackage ./nix/acme-sh-patched.nix {};
        acme-distributor = pkgs.callPackage ./nix/acme-distributor.nix {
          inherit acme-sh-patched;
          src = self;
        };
        acme-distributor-client = pkgs.callPackage ./nix/acme-distributor-client.nix {
          src = self;
        };
      in {
        packages = {
          inherit acme-distributor acme-distributor-client acme-sh-patched;
          default = acme-distributor;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.diesel-cli
            pkgs.openssl
            pkgs.sqlite
            acme-sh-patched
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      } // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
        checks.integration = import ./test.nix { inherit self pkgs; };
      }
    ) // {
      # NixOS modules
      nixosModules = {
        acme-distributor = import ./module.nix;
        acme-shim = import ./module-client.nix;
      };

      # Overlay for use in other flakes
      overlays.default = final: prev: {
        acme-sh-patched = prev.callPackage ./nix/acme-sh-patched.nix {};
        acme-distributor = prev.callPackage ./nix/acme-distributor.nix {
          inherit (final) acme-sh-patched;
          src = self;
        };
        acme-distributor-client = prev.callPackage ./nix/acme-distributor-client.nix {
          src = self;
        };
      };
    };
}
