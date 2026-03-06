{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.acme-distributor;
  acme-distributor = pkgs.acme-distributor;
in
{
  options = {
    services.acme-distributor = {
      enable = mkEnableOption "acme-distributor";

      port = mkOption {
        description = "Port to listen at";
        type = types.int;
        default = 3444;
      };

      config = mkOption {
        description = "configuration";
        type = types.attrs;
      };

      openFirewall = mkOption {
        type = types.bool;
        default = false;
        description = "Open ports in the firewall for acme-distributor.";
      };
    };
  };

  config = mkIf (cfg.enable) {
    services.acme-distributor.config = {
      server = {
        host = "::";
        port = cfg.port;
      };

      state = "/var/lib/acme-distributor";
    };

    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };

    systemd.services.acme-distributor = with pkgs; let
      configFile = pkgs.writeText "config.yaml" (builtins.toJSON cfg.config);
    in {
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      requires = [ "network-online.target" ];

      description = "acme-distributor";

      serviceConfig = {
        Type = "simple";
        DynamicUser = true;
        StateDirectory = "acme-distributor";
        ExecStart = "${acme-distributor}/bin/acme-distributor ${configFile}";
      };
    };
  };
}
