{ lib
, rustPlatform
, makeWrapper
, openssl
, sqlite
, pkg-config
, acme-sh-patched
, darwin
, stdenv
, src
}:

rustPlatform.buildRustPackage {
  pname = "acme-distributor";
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

  nativeBuildInputs = [ pkg-config makeWrapper ];

  cargoBuildFlags = [ "-p" "acme-distributor-server" ];

  postFixup = ''
    wrapProgram $out/bin/acme-distributor \
      --prefix PATH : ${lib.makeBinPath [ acme-sh-patched ]}
  '';

  meta = with lib; {
    description = "ACME certificate distributor server";
    license = licenses.mit;
  };
}
