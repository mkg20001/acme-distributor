{ self, pkgs, ... }:

let
  # Generate CA certificate and key for testing
  caSetup = pkgs.runCommand "ca-setup" {
    buildInputs = [ pkgs.openssl ];
  } ''
    mkdir -p $out
    openssl genrsa -out $out/ca.key 2048
    openssl req -new -x509 -days 365 -key $out/ca.key -out $out/ca.crt \
      -subj "/CN=Test CA/O=Test/C=US"
  '';

  testToken = "test-token-12345";

in pkgs.testers.nixosTest {
  name = "acme-distributor";

  nodes = {
    server = { config, pkgs, ... }: {
      imports = [ self.nixosModules.acme-distributor ];

      nixpkgs.overlays = [ self.overlays.default ];

      networking.firewall.allowedTCPPorts = [ 3444 ];

      services.acme-distributor = {
        enable = true;
        port = 3444;
        config = {
          providers = {
            local-ca = {
              type = "ca";
              key = "${caSetup}/ca.key";
              cert = "${caSetup}/ca.crt";
            };
          };
          certificates = {
            localhost = {
              names = [ "localhost" "client" ];
              provider = "local-ca";
            };
          };
          tokens = [
            { plain = testToken; }
          ];
        };
      };
    };

    client = { config, pkgs, ... }: {
      imports = [ self.nixosModules.acme-shim ];

      nixpkgs.overlays = [ self.overlays.default ];

      networking.firewall.allowedTCPPorts = [ 80 443 ];

      # Trust the test CA
      security.pki.certificateFiles = [ "${caSetup}/ca.crt" ];

      security.acme = {
        distributor-server = "http://server:3444";
        distributor-token = testToken;
        certs."localhost" = {
          domain = "localhost";
        };
      };

      services.nginx = {
        enable = true;
        virtualHosts."localhost" = {
          enableACME = true;
          forceSSL = true;
          locations."/" = {
            return = "200 'Hello from HTTPS!'";
            extraConfig = ''
              add_header Content-Type text/plain;
            '';
          };
        };
      };
    };
  };

  testScript = ''
    start_all()

    # Wait for acme-distributor server to be ready
    server.wait_for_unit("acme-distributor.service")
    server.wait_for_open_port(3444)

    # Verify server is responding
    server.succeed("curl -f http://localhost:3444/.well-known/acme-challenge/test || true")

    # Wait for client services
    client.wait_for_unit("nginx.service")

    # Trigger certificate fetch
    client.succeed("systemctl start acme-localhost.service")

    # Wait a bit for the certificate to be issued
    client.wait_for_file("/var/lib/acme/localhost/fullchain.pem")

    # Reload nginx to pick up the new certificate
    client.succeed("systemctl reload nginx.service")

    # Test HTTPS connection
    client.succeed("curl -f https://localhost/")
  '';
}
