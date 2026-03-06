{ config, lib, pkgs, options, ... }:

with lib;

let
  cfg = config.security.acme;

  # our values
  credentials = pkgs.writeText "acme.txt" cfg.distributor-token;
  # end our values

  # Used to make unique paths for each cert/account config set
  mkHash = with builtins; val: substring 0 20 (hashString "sha256" val);
  mkAccountHash = acmeServer: data: mkHash "${toString acmeServer} ${data.keyType} ${data.email}";
  accountDirRoot = "/var/lib/acme/.lego/accounts/";

  certToConfig = cert: data: let
    acmeServer = if data.server != null then data.server else cfg.server;

    # Minica and lego have a "feature" which replaces * with _. We need
    # to make this substitution to reference the output files from both programs.
    # End users never see this since we rename the certs.
    keyName = builtins.replaceStrings ["*"] ["_"] data.domain;

    # FIXME when mkChangedOptionModule supports submodules, change to that.
    # This is a workaround
    extraDomains = data.extraDomainNames ++ (
      optionals
      (data.extraDomains != "_mkMergedOptionModule")
      (builtins.attrNames data.extraDomains)
    );

    domainHash = mkHash "${concatStringsSep " " extraDomains} ${data.domain}";
    accountHash = (mkAccountHash acmeServer data);
    accountDir = accountDirRoot + accountHash;


  in {
    renewService = {
      description = mkForce "Renew ACME DISTRIBUTOR certificate for ${cert}";

      # path = with pkgs; [ acme-distributor-client ];

      # Working directory will be /tmp
      script = mkForce ''
        set -euxo pipefail

        echo '${domainHash}' > domainhash.txt

        mkdir -p certificates
        #                       <domain $1>      <state loc $2> <credential path $3> <server $4>    <cert $5>    <key $6>                      <ca $7>                              <fullchain $8>
        if ${pkgs.acme-distributor-client}/bin/acme-distributor-client check --state "$PWD/certificates/${keyName}.json"; then
          ${pkgs.acme-distributor-client}/bin/acme-distributor-client fetch --domain "${data.domain}" --state "$PWD/certificates/${keyName}.json" --credential "${credentials}" --server-url "${cfg.distributor-server}" --out-cert "/dev/null" --out-key "certificates/${keyName}.key" --out-ca "certificates/${keyName}.issuer.crt" --out-chain "certificates/${keyName}.crt"
        fi

        # mv domainhash.txt certificates/
        chmod 640 certificates/*

        # Group might change between runs, re-apply it
        chown 'acme:${data.group}' certificates/*

        # Copy all certs to the "real" certs directory
        CERT='certificates/${keyName}.crt'
        if [ -e "$CERT" ] && ! cmp -s "$CERT" out/fullchain.pem; then
          touch out/renewed
          echo Installing new certificate
          cp -vp 'certificates/${keyName}.crt' out/fullchain.pem
          cp -vp 'certificates/${keyName}.key' out/key.pem
          cp -vp 'certificates/${keyName}.issuer.crt' out/chain.pem
          ln -sf fullchain.pem out/cert.pem
          cat out/key.pem out/fullchain.pem > out/full.pem
        fi
      '';
    };
  };

  certConfigs = mapAttrs certToConfig cfg.certs;

in {

  options = {
    security.acme = {

      distributor-server = mkOption {
        type = types.str;
        description = "acme-distributor server";
      };

      distributor-token = mkOption {
        type = types.str;
        description = "token for acme-distributor server";
      };
    };

    services.nginx.virtualHosts = {
      apply = hosts:
        (mapAttrs (key: vhost:
           if vhost.acmeFallbackHost != null then vhost
           else vhost // { acmeFallbackHost = elemAt (splitString "/" cfg.distributor-server) 2; }
        )) hosts;
      _type = "option";
    };
  };

  config = mkMerge [
    (mkIf (cfg.certs != { }) {

      # set those values as we aren't using letsencrypt (so we can do this)
      # and it triggers assertions if we don't
      security.acme.acceptTerms = mkDefault true;
      security.acme.defaults.email = mkDefault "me@localhost";

      systemd.services = (mapAttrs' (cert: conf: nameValuePair "acme-${cert}" conf.renewService) certConfigs) // (mapAttrs' (cert: conf: nameValuePair "acme-renew-${cert}" conf.renewService) certConfigs) // (mapAttrs' (cert: conf: nameValuePair "acme-order-renew-${cert}" conf.renewService) certConfigs);
    })
  ];
}
