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

