use acme_distributor_common::{IssuanceResult, ProviderConfig};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{debug, info};

use super::traits::{ChallengeStore_, Provider, ProviderError};

pub struct AcmeShProvider {
    id: String,
    state_path: PathBuf,
    dns_plugin: Option<String>,
    env_vars: HashMap<String, String>,
}

impl AcmeShProvider {
    pub fn new(id: &str, config: &ProviderConfig, state_path: &Path) -> Self {
        let provider_state = state_path.join(id);
        std::fs::create_dir_all(&provider_state).ok();

        Self {
            id: id.to_string(),
            state_path: provider_state,
            dns_plugin: config.dns.clone(),
            env_vars: config.env.clone().unwrap_or_default(),
        }
    }

    async fn parse_cert_expiry(&self, cert_path: &Path) -> Result<i64, ProviderError> {
        let cert_pem = tokio::fs::read(cert_path).await?;
        let (_, pem) = x509_parser::pem::parse_x509_pem(&cert_pem)
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
impl Provider for AcmeShProvider {
    async fn issue(
        &self,
        id: &str,
        names: &[String],
        _challenge_store: ChallengeStore_,
    ) -> Result<IssuanceResult, ProviderError> {
        info!("Issuing certificate {} for domains: {:?}", id, names);

        let mut args = vec![
            "--config-home".to_string(),
            self.state_path.display().to_string(),
            "--server".to_string(),
            "letsencrypt".to_string(),
            "--issue".to_string(),
        ];

        // Add DNS or webroot mode
        if let Some(ref dns) = self.dns_plugin {
            args.extend(["--dns".to_string(), dns.clone()]);
        } else {
            let webroot = self.state_path.join(".webroot");
            tokio::fs::create_dir_all(&webroot).await?;
            args.extend(["--webroot".to_string(), webroot.display().to_string()]);
        }

        // Add domains
        for name in names {
            args.extend(["-d".to_string(), name.clone()]);
        }

        // Build environment
        let mut env: HashMap<String, String> = self.env_vars.clone();
        env.insert("DOMAIN_CERT_ID".to_string(), id.to_string());
        // Clear SUDO env vars
        for key in ["SUDO_COMMAND", "SUDO_USER", "SUDO_UID", "SUDO_GID"] {
            env.insert(key.to_string(), String::new());
        }

        debug!("Running acme.sh with args: {:?}", args);

        // Note: For DNS validation, acme.sh handles everything via the DNS plugin.
        // For webroot validation, challenges are written to files in the webroot directory.
        // We don't need to capture fd 3 for most use cases.

        let output = Command::new("acme.sh")
            .args(&args)
            .envs(&env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await
            .map_err(|e| ProviderError::IssuanceFailed(format!("Failed to spawn acme.sh: {}", e)))?;

        if !output.success() {
            return Err(ProviderError::IssuanceFailed(format!(
                "acme.sh exited with status: {}",
                output
            )));
        }

        // Find certificate directory (check for _ecc suffix)
        let mut cert_dir = self.state_path.join(id);
        let ecc_dir = self.state_path.join(format!("{}_ecc", id));
        if ecc_dir.exists() {
            cert_dir = ecc_dir;
        }

        // Read certificate files
        let cert_pem = tokio::fs::read_to_string(cert_dir.join(format!("{}.cer", id))).await?;
        let key_pem = tokio::fs::read_to_string(cert_dir.join(format!("{}.key", id))).await?;
        let ca_pem = tokio::fs::read_to_string(cert_dir.join("ca.cer")).await?;
        let chain_pem = tokio::fs::read_to_string(cert_dir.join("fullchain.cer")).await?;

        // Parse expiry
        let expires_at = self
            .parse_cert_expiry(&cert_dir.join(format!("{}.cer", id)))
            .await?;

        // Set prefer_renew_before to 1 week before expiry
        let one_week_ms: i64 = 7 * 24 * 60 * 60 * 1000;
        let prefer_renew_before = Some(expires_at - one_week_ms);

        info!("Certificate {} issued successfully, expires at {}", id, expires_at);

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
