#!/bin/bash
set -e

CA_DIR="ca"
CA_KEY="$CA_DIR/ca.key"
CA_CERT="$CA_DIR/ca.crt"
DAYS=3650  # 10 years

mkdir -p "$CA_DIR"

echo "Generating CA private key..."
openssl genrsa -out "$CA_KEY" 4096

echo "Generating CA certificate..."
openssl req -new -x509 -days "$DAYS" -key "$CA_KEY" -out "$CA_CERT" \
    -subj "/CN=ACME Distributor CA/O=Local Development/C=US"

echo "CA generated successfully:"
echo "  Key:  $CA_KEY"
echo "  Cert: $CA_CERT"
