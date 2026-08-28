# acme-distributor

Service that hands out certificates to machines requesting them.

This allows you to acquire all certificates through one central server, keeping your DNS credentials safe, while giving you full flexibility over the certificates being used.

What it can do:
- Acquire Let's Encrypt certificates using DNS challenge and distribute them
- Sign certificates with your own CA
- Distribute custom, manually managed certificates
- Be extended into supporting whatever CA and certificate acquisition API you wish

## Providers

### acme-sh (Let's Encrypt)

Issues certificates from Let's Encrypt (or other ACME CAs) using [acme.sh](https://github.com/acmesh-official/acme.sh) with DNS validation.

```yaml
providers:
  letsencrypt:
    type: acme-sh
    dns: dns_cf  # DNS plugin (see acme.sh documentation)
    env:
      CF_Key: your-cloudflare-api-key
      CF_Email: your-email@example.com

certificates:
  example.com:
    names:
      - example.com
      - "*.example.com"
    provider: letsencrypt
```

Supported DNS plugins include: `dns_cf` (Cloudflare), `dns_aws` (Route53), `dns_gcloud` (Google Cloud DNS), and [many more](https://github.com/acmesh-official/acme.sh/wiki/dnsapi).

### acme-lib (Let's Encrypt HTTP-01)

Issues certificates from Let's Encrypt using HTTP-01 validation via the [acme-lib](https://crates.io/crates/acme-lib) crate. Requires the server to be reachable on port 80.

```yaml
providers:
  letsencrypt-http:
    type: acme-lib
    env:
      email: your-email@example.com
      staging: 'true'  # optional: use staging for testing

certificates:
  example.com:
    names:
      - example.com
      - www.example.com
    provider: letsencrypt-http
```

The HTTP-01 challenge requires that `/.well-known/acme-challenge/` requests reach the acme-distributor server.

### ca (Local CA)

Signs certificates using your own Certificate Authority. Useful for internal services and development environments.

```yaml
providers:
  internal-ca:
    type: ca
    key: /path/to/ca.key   # CA private key (PEM)
    cert: /path/to/ca.crt  # CA certificate (PEM)

certificates:
  internal.example.com:
    names:
      - internal.example.com
      - "*.internal.example.com"
    provider: internal-ca
```

Generate a CA with:
```bash
./generate-ca.sh  # Creates ca/ca.key and ca/ca.crt
```

### file (Local Files)

Reads certificates from local files. Useful for manually managed certificates or certificates obtained through other means.

```yaml
providers:
  manual:
    type: file

certificates:
  external.example.com:
    names:
      - external.example.com
    provider: manual
```

Directory structure:
```
{state}/{provider-id}/{cert-id}/
├── cert.pem   # End-entity certificate (required)
├── key.pem    # Private key (required)
├── ca.pem     # CA certificate (optional)
└── chain.pem  # Full chain: cert + intermediates + CA (required)
```

## Configuration

See [config.example.yaml](config.example.yaml) for a complete configuration example.

## NixOS Modules

This flake provides two NixOS modules:

### Server Module (`nixosModules.acme-distributor`)

Runs the acme-distributor server:

```nix
{
  imports = [ acme-distributor.nixosModules.acme-distributor ];

  services.acme-distributor = {
    enable = true;
    port = 3444;
    config = {
      providers = { /* ... */ };
      certificates = { /* ... */ };
      tokens = [ { plain = "secret-token"; } ];
    };
  };
}
```

### Client Shim Module (`nixosModules.acme-shim`)

Replaces the standard NixOS `security.acme` module with acme-distributor. This allows you to use `security.acme.certs` and `services.nginx` with `enableACME = true` as usual, but certificates are fetched from your acme-distributor server instead of Let's Encrypt directly.

```nix
{
  imports = [ acme-distributor.nixosModules.acme-shim ];

  # Configure the acme-distributor server URL and token
  security.acme = {
    distributor-server = "http://acme-server.internal:3444";
    distributor-token = "secret-token";

    # Use security.acme.certs as normal
    certs."example.com" = {
      domain = "example.com";
    };
  };

  # nginx with enableACME works as expected
  services.nginx.virtualHosts."example.com" = {
    enableACME = true;
    forceSSL = true;
  };
}
```

**Key features:**

- **Drop-in replacement**: Works with existing `security.acme.certs` configuration
- **Automatic challenge routing**: When using nginx, `/.well-known/acme-challenge/` requests are automatically proxied to the acme-distributor server via `acmeFallbackHost`
- **Systemd integration**: Replaces `acme-*` systemd services to use the acme-distributor client

### Using the Overlay

```nix
{
  nixpkgs.overlays = [ acme-distributor.overlays.default ];

  # Packages available: pkgs.acme-distributor, pkgs.acme-distributor-client
  environment.systemPackages = [ pkgs.acme-distributor-client ];
}
```

## MikroTik RouterOS

[contrib/routeros.rsc](contrib/routeros.rsc) fetches a certificate once a day and installs it as the router's TLS certificate (`www-ssl`, `api-ssl`).

Edit `$Server`, `$Token` and `$Domain` at the top, upload the file to the router and run:

```
/system/script/add name=acme-distributor policy=read,write,policy,test \
  source=[/file/get [find where name="routeros.rsc"] contents]
/system/scheduler/add name=acme-distributor interval=1d start-time=03:00:00 \
  policy=read,write,policy,test on-event="/system/script/run acme-distributor"
```

It only re-imports when the certificate changed, so the daily run is a no-op in between renewals. Requires RouterOS 7 (uses `:deserialize from=json`), a working clock and DNS, and that the router trusts the CA of the acme-distributor server.

## Development

```sh
cargo watch -- cargo run --bin acme-distributor
```

