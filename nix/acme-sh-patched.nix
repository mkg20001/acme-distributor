{ acme-sh, fetchFromGitHub }:

acme-sh.overrideAttrs (p: p // {
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
})
