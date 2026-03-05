{
  description = "acme-distributor";

  inputs.nixpkgs.url = "https://git.xeredo.it/xeredo/nixpkgs/-/jobs/artifacts/xeredo/raw/nixpkgs.tar.xz?job=build";
  inputs.nix-node-package.url = "github:mkg20001/nix-node-package/master";

  outputs = { self, nixpkgs, nix-node-package }:

    let
      supportedSystems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);
    in

    {
      overlay = final: prev: {
        acme-distributor = prev.callPackage ./package.nix {
          mkNode = nix-node-package.lib.nix-node-package prev;
        };
        acme-distributor-client = prev.callPackage ./client.nix {};
      };

      defaultPackage = forAllSystems (system: (import nixpkgs {
        inherit system;
        overlays = [ self.overlay ];
      }).acme-distributor);

      nixosModules.acme-shim = import ./module-client.nix;
      nixosModules.acme-distributor = import ./module.nix;

    };
}
