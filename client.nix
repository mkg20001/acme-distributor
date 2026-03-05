{ stdenv
, lib
, drvSrc ? ./client
, makeWrapper
, jq
, curl
}:

let
  extraPath = [
    jq
    curl
  ];
in
stdenv.mkDerivation {
  pname = "acme-distributor-client";
  version = "unstable";

  src = drvSrc;

  installPhase = ''
    install -D client.sh $out/bin/acme-distributor-client
  '';

  buildInputs = extraPath ++ [

  ];

  inherit extraPath;

  nativeBuildInputs = [
    makeWrapper
  ];

  preFixup = ''
    for bin in $out/bin/*; do
      wrapProgram $bin --prefix PATH : ${lib.makeBinPath extraPath}
    done
  '';
}
