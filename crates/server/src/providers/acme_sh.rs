use acme_distributor_common::{IssuanceResult, ProviderConfig};
use async_trait::async_trait;
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
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
        challenge_store: ChallengeStore_,
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

        // Create pipe for fd 3 to receive challenge tokens from acme.sh
        let (pipe_read, pipe_write) = os_pipe::pipe()
            .map_err(|e| ProviderError::IssuanceFailed(format!("Failed to create pipe: {}", e)))?;

        // Spawn acme.sh with fd 3 connected to our pipe
        let mut child = unsafe {
            Command::new("acme.sh")
                .args(&args)
                .envs(&env)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .pre_exec(move || {
                    // Duplicate pipe_write to fd 3
                    let write_fd = pipe_write.as_raw_fd();
                    if write_fd != 3 {
                        libc::dup2(write_fd, 3);
                    }
                    Ok(())
                })
                .spawn()
                .map_err(|e| {
                    ProviderError::IssuanceFailed(format!("Failed to spawn acme.sh: {}", e))
                })?
        };

        // Convert pipe reader to async and spawn task to read challenge tokens
        let async_reader = tokio::fs::File::from_std(unsafe {
            std::fs::File::from_raw_fd(pipe_read.into_raw_fd())
        });
        let reader = BufReader::new(async_reader);
        let mut lines = reader.lines();

        let challenge_store_clone = challenge_store.clone();
        let reader_task = tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                // Line format: "keyauthorization" where token is the part before the dot
                let token = line.split('.').next().unwrap_or(&line);
                info!("Challenge token={}, auth={}", token, line);
                challenge_store_clone
                    .write()
                    .await
                    .insert(token.to_string(), line);
            }
        });

        // Wait for acme.sh to complete
        let status = child.wait().await.map_err(|e| {
            ProviderError::IssuanceFailed(format!("Failed to wait for acme.sh: {}", e))
        })?;

        // Wait for reader task to finish
        let _ = reader_task.await;

        if !status.success() {
            return Err(ProviderError::IssuanceFailed(format!(
                "acme.sh exited with status: {}",
                status
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

        info!(
            "Certificate {} issued successfully, expires at {}",
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
