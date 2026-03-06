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
          sqlite
        ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.darwin.apple_sdk.frameworks.Security
          pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
        ];

        nativeBuildInputs = with pkgs; [
          pkg-config
          rustToolchain
        ];

        acme-sh = with pkgs; pkgs.acme-sh.overrideAttrs(p: p // {
          preFixup = ''
            sed 's|$CERT_HOME/$domain|$CERT_HOME/$DOMAIN_CERT_ID|g' -i $out/libexec/acme.sh
            sed 's|$DOMAIN_PATH/$domain|$DOMAIN_PATH/$DOMAIN_CERT_ID|g' -i $out/libexec/acme.sh
            sed 's|RENEW_SKIP=2|RENEW_SKIP=0|g' -i $out/libexec/acme.sh
            sed 's|printf "%s" "$keyauthorization" >"$wellknown_path/$token"|echo "$keyauthorization" >\&3 \&\& printf "%s" "$keyauthorization" >"$wellknown_path/$token"|' -i $out/libexec/acme.sh
          '';

          version = "unstable";

          src = fetchFromGitHub {
            owner = "acmesh-official";
            repo = "acme.sh";
            rev = "d4befeb5360e278fbc6be775c0ec60de464ed9fb";
            sha256 = "VW31SLy8IAMpkWeh+s9xT1KLLXf/jcJCr8MSmL2FK7Q=";
            fetchSubmodules = false;
          };
        });

        # Server package
        acme-distributor = pkgs.rustPlatform.buildRustPackage {
          pname = "acme-distributor";
          version = "0.1.0";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit buildInputs;
          nativeBuildInputs = nativeBuildInputs ++ [ pkgs.makeWrapper ];

          # Only build the server binary
          cargoBuildFlags = [ "-p" "acme-distributor-server" ];

          postFixup = ''
            wrapProgram $out/bin/acme-distributor \
              --prefix PATH : ${pkgs.lib.makeBinPath [ acme-sh ]}
          '';

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
            acme-sh
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
        acme-distributor = self.packages.${prev.system}.acme-distributor;
        acme-distributor-client = self.packages.${prev.system}.acme-distributor-client;
      };
    };
}
