use acme_distributor_common::{IssuanceResult, ProviderConfig};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::info;
use x509_parser::prelude::*;

use super::traits::{ChallengeStore_, Provider, ProviderError};

/// Provider that reads certificates from local files.
///
/// Directory structure:
///   {state_path}/{provider_id}/{cert_id}/
///     - cert.pem     (end-entity certificate)
///     - key.pem      (private key)
///     - ca.pem       (CA certificate, optional)
///     - chain.pem    (fullchain: cert + intermediates + CA)
pub struct FileProvider {
    id: String,
    state_path: PathBuf,
}

impl FileProvider {
    pub fn new(id: &str, _config: &ProviderConfig, state_path: &Path) -> Self {
        let provider_state = state_path.join(id);
        std::fs::create_dir_all(&provider_state).ok();

        Self {
            id: id.to_string(),
            state_path: provider_state,
        }
    }

    fn cert_dir(&self, cert_id: &str) -> PathBuf {
        self.state_path.join(cert_id)
    }

    async fn parse_cert_expiry(&self, cert_pem: &str) -> Result<i64, ProviderError> {
        let (_, pem) = parse_x509_pem(cert_pem.as_bytes())
            .map_err(|e| ProviderError::ParseError(format!("Failed to parse PEM: {:?}", e)))?;
        let cert = pem
            .parse_x509()
            .map_err(|e| ProviderError::ParseError(format!("Failed to parse X509: {:?}", e)))?;

        // timestamp() returns seconds, convert to milliseconds
        let expiry = cert.validity().not_after.timestamp() * 1000;
        Ok(expiry)
    }
}

#[async_trait]
impl Provider for FileProvider {
    async fn issue(
        &self,
        id: &str,
        _names: &[String],
        _challenge_store: ChallengeStore_,
    ) -> Result<IssuanceResult, ProviderError> {
        let cert_dir = self.cert_dir(id);

        info!("Reading certificate {} from {:?}", id, cert_dir);

        if !cert_dir.exists() {
            return Err(ProviderError::IssuanceFailed(format!(
                "Certificate directory does not exist: {:?}",
                cert_dir
            )));
        }

        // Read certificate files
        let cert_path = cert_dir.join("cert.pem");
        let key_path = cert_dir.join("key.pem");
        let ca_path = cert_dir.join("ca.pem");
        let chain_path = cert_dir.join("chain.pem");

        let cert_pem = tokio::fs::read_to_string(&cert_path).await.map_err(|e| {
            ProviderError::IssuanceFailed(format!("Failed to read cert.pem: {}", e))
        })?;

        let key_pem = tokio::fs::read_to_string(&key_path)
            .await
            .map_err(|e| ProviderError::IssuanceFailed(format!("Failed to read key.pem: {}", e)))?;

        // CA is optional - use empty string if not present
        let ca_pem = tokio::fs::read_to_string(&ca_path)
            .await
            .unwrap_or_default();

        // Chain is required
        let chain_pem = tokio::fs::read_to_string(&chain_path).await.map_err(|e| {
            ProviderError::IssuanceFailed(format!("Failed to read chain.pem: {}", e))
        })?;

        // Parse expiry from certificate
        let expires_at = self.parse_cert_expiry(&cert_pem).await?;

        // Set prefer_renew_before to 1 week before expiry
        let one_week_ms: i64 = 7 * 24 * 60 * 60 * 1000;
        let prefer_renew_before = Some(expires_at - one_week_ms);

        info!(
            "Certificate {} loaded successfully, expires at {}",
            id, expires_at
        );

        Ok(IssuanceResult {
            cert_pem,
            key_pem,
            ca_pem,
            chain_pem,
            expires_at,
            prefer_renew_before,
        })
    }

    fn id(&self) -> &str {
        &self.id
    }
}
