use acme_distributor_common::{IssuanceResult, ProviderConfig};
use acme_lib::create_p384_key;
use acme_lib::persist::FilePersist;
use acme_lib::{Directory, DirectoryUrl};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info};
use x509_parser::prelude::*;

use super::traits::{ChallengeStore_, Provider, ProviderError};

pub struct AcmeLibProvider {
    id: String,
    state_path: PathBuf,
    staging: bool,
    email: String,
}

impl AcmeLibProvider {
    pub fn new(id: &str, config: &ProviderConfig, state_path: &Path) -> Self {
        let provider_state = state_path.join(id);
        std::fs::create_dir_all(&provider_state).ok();

        // Get email from env config, default to empty
        let email = config
            .env
            .as_ref()
            .and_then(|e| e.get("email").cloned())
            .unwrap_or_default();

        // Check if staging mode is enabled
        let staging = config
            .env
            .as_ref()
            .and_then(|e| e.get("staging"))
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false);

        Self {
            id: id.to_string(),
            state_path: provider_state,
            staging,
            email,
        }
    }

    fn parse_cert_expiry(&self, cert_pem: &str) -> Result<i64, ProviderError> {
        let (_, pem) = parse_x509_pem(cert_pem.as_bytes())
            .map_err(|e| ProviderError::ParseError(format!("Failed to parse PEM: {:?}", e)))?;
        let cert = pem
            .parse_x509()
            .map_err(|e| ProviderError::ParseError(format!("Failed to parse X509: {:?}", e)))?;

        let expiry = cert.validity().not_after.timestamp() * 1000;
        Ok(expiry)
    }
}

#[async_trait]
impl Provider for AcmeLibProvider {
    async fn issue(
        &self,
        id: &str,
        names: &[String],
        challenge_store: ChallengeStore_,
    ) -> Result<IssuanceResult, ProviderError> {
        info!("Issuing certificate {} for domains: {:?}", id, names);

        let primary_domain = names
            .first()
            .ok_or_else(|| ProviderError::IssuanceFailed("No domains provided".to_string()))?
            .clone();

        let alt_names: Vec<String> = names.iter().skip(1).cloned().collect();
        let state_path = self.state_path.clone();
        let staging = self.staging;
        let email = self.email.clone();
        let challenge_store_clone = challenge_store.clone();

        // acme-lib is synchronous, run in blocking task
        let result = tokio::task::spawn_blocking(move || {
            // Set up persistence
            let persist = FilePersist::new(&state_path);

            // Select directory URL
            let url = if staging {
                DirectoryUrl::LetsEncryptStaging
            } else {
                DirectoryUrl::LetsEncrypt
            };

            debug!("Connecting to ACME directory: {:?}", url);
            debug!("Account state path: {:?}", state_path);

            // Create directory and account
            // FilePersist stores account credentials in the state directory
            // and reuses them automatically on subsequent calls
            let dir = Directory::from_url(persist, url)
                .map_err(|e| ProviderError::IssuanceFailed(format!("Directory error: {}", e)))?;

            // This will load existing account from disk or create a new one
            let acc = dir
                .account(&email)
                .map_err(|e| ProviderError::IssuanceFailed(format!("Account error: {}", e)))?;

            info!("Using ACME account for email: {}", email);

            // Create order
            let alt_refs: Vec<&str> = alt_names.iter().map(|s| s.as_str()).collect();
            let mut ord_new = acc
                .new_order(&primary_domain, &alt_refs)
                .map_err(|e| ProviderError::IssuanceFailed(format!("Order error: {}", e)))?;

            // Process authorizations
            let ord_csr = loop {
                if let Some(ord_csr) = ord_new.confirm_validations() {
                    break ord_csr;
                }

                let auths = ord_new
                    .authorizations()
                    .map_err(|e| ProviderError::IssuanceFailed(format!("Auth error: {}", e)))?;

                for auth in auths {
                    let chall = auth.http_challenge();
                    let token = chall.http_token().to_string();
                    let proof = chall.http_proof();

                    debug!("HTTP challenge: token={}, proof={}", token, proof);

                    // Store challenge in challenge store (need to use blocking)
                    let challenge_store_inner = challenge_store_clone.clone();
                    let token_clone = token.clone();
                    let proof_clone = proof.clone();

                    // Use std::thread to avoid async in sync context
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(async {
                            challenge_store_inner
                                .write()
                                .await
                                .insert(token_clone, proof_clone);
                        });
                    })
                    .join()
                    .map_err(|_| {
                        ProviderError::IssuanceFailed("Failed to store challenge".to_string())
                    })?;

                    // Wait a moment for challenge to propagate
                    std::thread::sleep(Duration::from_secs(1));

                    // Validate challenge
                    chall
                        .validate(5000)
                        .map_err(|e| ProviderError::IssuanceFailed(format!("Validation error: {}", e)))?;
                }

                // Refresh order state
                ord_new
                    .refresh()
                    .map_err(|e| ProviderError::IssuanceFailed(format!("Refresh error: {}", e)))?;
            };

            // Generate private key and finalize order
            let pkey = create_p384_key();
            let ord_cert = ord_csr
                .finalize_pkey(pkey, 5000)
                .map_err(|e| ProviderError::IssuanceFailed(format!("Finalize error: {}", e)))?;

            // Download certificate
            let cert = ord_cert
                .download_and_save_cert()
                .map_err(|e| ProviderError::IssuanceFailed(format!("Download error: {}", e)))?;

            Ok::<_, ProviderError>((
                cert.certificate().to_string(),
                cert.private_key().to_string(),
            ))
        })
        .await
        .map_err(|e| ProviderError::IssuanceFailed(format!("Task error: {}", e)))??;

        let (cert_pem, key_pem) = result;

        // Parse the certificate chain
        // acme-lib returns the full chain in certificate()
        let chain_pem = cert_pem.clone();

        // Extract just the first certificate for cert_pem
        let first_cert_end = cert_pem
            .find("-----END CERTIFICATE-----")
            .map(|i| i + "-----END CERTIFICATE-----".len())
            .unwrap_or(cert_pem.len());
        let cert_only = cert_pem[..first_cert_end].to_string();

        // Extract CA (everything after the first cert)
        let ca_pem = if first_cert_end < cert_pem.len() {
            cert_pem[first_cert_end..].trim().to_string()
        } else {
            String::new()
        };

        // Parse expiry
        let expires_at = self.parse_cert_expiry(&cert_only)?;

        // Set prefer_renew_before to 1 week before expiry
        let one_week_ms: i64 = 7 * 24 * 60 * 60 * 1000;
        let prefer_renew_before = Some(expires_at - one_week_ms);

        info!(
            "Certificate {} issued successfully, expires at {}",
            id, expires_at
        );

        Ok(IssuanceResult {
            cert_pem: cert_only,
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
