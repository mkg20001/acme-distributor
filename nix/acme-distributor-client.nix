{ lib
, rustPlatform
, openssl
, sqlite
, pkg-config
, darwin
, stdenv
, src
}:

rustPlatform.buildRustPackage {
  pname = "acme-distributor-client";
  version = "0.1.0";

  inherit src;

  cargoLock.lockFile = "${src}/Cargo.lock";

  buildInputs = [
    openssl
    sqlite
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.apple_sdk.frameworks.Security
    darwin.apple_sdk.frameworks.SystemConfiguration
  ];

  nativeBuildInputs = [ pkg-config ];

  cargoBuildFlags = [ "-p" "acme-distributor-client" ];

  meta = with lib; {
    description = "ACME certificate distributor client";
    license = licenses.mit;
  };
}
