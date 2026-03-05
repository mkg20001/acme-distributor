use acme_distributor_common::{IssuanceResult, ProviderConfig};
use async_trait::async_trait;
use chrono::Datelike;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use std::path::PathBuf;
use tracing::info;

use super::traits::{ChallengeStore_, Provider, ProviderError};

pub struct CaProvider {
    id: String,
    ca_key_path: PathBuf,
    ca_cert_path: PathBuf,
}

impl CaProvider {
    pub fn new(id: &str, config: &ProviderConfig) -> Self {
        let ca_key_path = config
            .key
            .as_ref()
            .map(PathBuf::from)
            .expect("CA provider requires 'key' config");
        let ca_cert_path = config
            .cert
            .as_ref()
            .map(PathBuf::from)
            .expect("CA provider requires 'cert' config");

        Self {
            id: id.to_string(),
            ca_key_path,
            ca_cert_path,
        }
    }
}

#[async_trait]
impl Provider for CaProvider {
    async fn issue(
        &self,
        id: &str,
        names: &[String],
        _challenge_store: ChallengeStore_,
    ) -> Result<IssuanceResult, ProviderError> {
        info!("CA signing certificate {} for domains: {:?}", id, names);

        // Load CA key and certificate PEM
        let ca_key_pem = tokio::fs::read_to_string(&self.ca_key_path).await?;
        let ca_cert_pem = tokio::fs::read_to_string(&self.ca_cert_path).await?;

        // Parse the CA key
        let ca_key = KeyPair::from_pem(&ca_key_pem)
            .map_err(|e| ProviderError::ConfigError(format!("Failed to parse CA key: {}", e)))?;

        // Create a CA certificate for signing purposes
        // We reconstruct it using the loaded key
        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "Internal CA");
        ca_params.distinguished_name = ca_dn;

        let ca_cert = ca_params
            .self_signed(&ca_key)
            .map_err(|e| ProviderError::ConfigError(format!("Failed to create CA cert: {}", e)))?;

        // Generate new key pair for the end-entity certificate
        let key_pair = KeyPair::generate()
            .map_err(|e| ProviderError::IssuanceFailed(format!("Failed to generate key: {}", e)))?;

        // Create certificate parameters with SANs
        let first_name = names.first().cloned().unwrap_or_else(|| "localhost".to_string());
        let mut params = CertificateParams::new(names.to_vec())
            .map_err(|e| ProviderError::IssuanceFailed(format!("Failed to create params: {}", e)))?;

        // Set distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, &first_name);
        params.distinguished_name = dn;

        // Set validity (90 days to match Let's Encrypt)
        let now = chrono::Utc::now();
        let not_before = now - chrono::Duration::hours(1); // 1 hour before to handle clock skew
        let not_after = now + chrono::Duration::days(90);

        params.not_before = rcgen::date_time_ymd(
            not_before.year(),
            not_before.month() as u8,
            not_before.day() as u8,
        );
        params.not_after = rcgen::date_time_ymd(
            not_after.year(),
            not_after.month() as u8,
            not_after.day() as u8,
        );

        // Sign certificate with CA
        let cert = params
            .signed_by(&key_pair, &ca_cert, &ca_key)
            .map_err(|e| ProviderError::IssuanceFailed(format!("Failed to sign certificate: {}", e)))?;

        let cert_pem = cert.pem();
        let key_pem = key_pair.serialize_pem();
        // Chain includes the end-entity cert and the original CA cert
        let chain_pem = format!("{}\n{}", cert_pem, ca_cert_pem);

        let expires_at = not_after.timestamp_millis();
        let one_week_ms: i64 = 7 * 24 * 60 * 60 * 1000;
        let prefer_renew_before = Some(expires_at - one_week_ms);

        info!(
            "Certificate {} signed successfully, expires at {}",
            id, expires_at
        );

        Ok(IssuanceResult {
            cert_pem,
            key_pem,
            ca_pem: ca_cert_pem,
            chain_pem,
            expires_at,
            prefer_renew_before,
        })
    }

    fn id(&self) -> &str {
        &self.id
    }
}
