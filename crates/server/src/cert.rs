use acme_distributor_common::CertificateConfig;
use diesel::SqliteConnection;
use glob_match::glob_match;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tracing::{info, warn};

use crate::challenge_store::SharedChallengeStore;
use crate::db::{self, Certificate, DbPool, NewCertificate};
use crate::providers::{Provider, ProviderError};

const SIXTY_DAYS_MS: i64 = 60 * 24 * 60 * 60 * 1000;

/// Reason why a certificate needs to be issued or renewed
#[derive(Debug)]
pub enum IssuanceReason {
    /// Certificate doesn't exist in the database
    Missing,
    /// Certificate has expired
    Expired,
    /// Provider was changed in config
    ProviderChanged { old: String, new: String },
    /// Domain names were changed in config
    NamesChanged,
    /// Certificate is within the renewal window
    InRenewalWindow,
}

impl fmt::Display for IssuanceReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "not found"),
            Self::Expired => write!(f, "expired"),
            Self::ProviderChanged { old, new } => {
                write!(f, "provider changed ({} -> {})", old, new)
            }
            Self::NamesChanged => write!(f, "names changed"),
            Self::InRenewalWindow => write!(f, "in renewal window"),
        }
    }
}

/// Reason why a certificate should be deleted
#[derive(Debug)]
pub enum DeletionReason {
    /// Certificate hasn't been requested in 60+ days and is not a named certificate
    Stale,
    /// The provider no longer exists in config
    MissingProvider,
    /// The source certificate config no longer exists
    MissingSourceConfig,
}

impl fmt::Display for DeletionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale => write!(f, "not requested in 60+ days"),
            Self::MissingProvider => write!(f, "provider no longer exists"),
            Self::MissingSourceConfig => write!(f, "source config no longer exists"),
        }
    }
}

/// Check if a certificate needs to be issued or renewed
pub fn check_needs_issuance(
    cert: Option<&Certificate>,
    config: &CertificateConfig,
    names: &[String],
) -> Option<IssuanceReason> {
    match cert {
        None => Some(IssuanceReason::Missing),
        Some(cert) if cert.is_expired() => Some(IssuanceReason::Expired),
        Some(cert) => {
            if cert.provider != config.provider {
                Some(IssuanceReason::ProviderChanged {
                    old: cert.provider.clone(),
                    new: config.provider.clone(),
                })
            } else if cert.get_names() != names {
                Some(IssuanceReason::NamesChanged)
            } else if cert.needs_renewal() {
                Some(IssuanceReason::InRenewalWindow)
            } else {
                None
            }
        }
    }
}

/// Check if a certificate should be deleted during maintenance
pub fn check_needs_deletion(
    cert: &Certificate,
    providers: &HashMap<String, Arc<dyn Provider>>,
    certificates: &HashMap<String, CertificateConfig>,
) -> Option<DeletionReason> {
    let now = chrono::Utc::now().timestamp_millis();
    let sixty_days_ago = now - SIXTY_DAYS_MS;

    // Delete if not requested in 60+ days and not a named certificate
    if cert.requested_at < sixty_days_ago && !certificates.contains_key(&cert.id) {
        return Some(DeletionReason::Stale);
    }

    // Delete if provider doesn't exist
    if !providers.contains_key(&cert.provider) {
        return Some(DeletionReason::MissingProvider);
    }

    // Delete if source certificate config doesn't exist
    if !certificates.contains_key(&cert.source) {
        return Some(DeletionReason::MissingSourceConfig);
    }

    None
}

/// Issue a certificate and save it to the database
/// If `requested_at` is None, uses the current timestamp
pub async fn issue_certificate(
    conn: &mut SqliteConnection,
    cert_id: &str,
    source: &str,
    provider: &Arc<dyn Provider>,
    provider_name: &str,
    names: &[String],
    challenge_store: SharedChallengeStore,
    requested_at: Option<i64>,
) -> Result<db::Certificate, ProviderError> {
    let result = provider.issue(cert_id, names, challenge_store).await?;

    let new_cert = NewCertificate {
        id: cert_id.to_string(),
        source: source.to_string(),
        provider: provider_name.to_string(),
        names: serde_json::to_string(names).unwrap(),
        cert_pem: Some(result.cert_pem),
        key_pem: Some(result.key_pem),
        ca_pem: Some(result.ca_pem),
        chain_pem: Some(result.chain_pem),
        expires_at: result.expires_at,
        prefer_renew_before: result.prefer_renew_before,
        prefer_renew_after: None,
        requested_at: requested_at.unwrap_or_else(|| chrono::Utc::now().timestamp_millis()),
    };

    db::upsert_certificate(conn, new_cert);
    Ok(db::get_certificate(conn, cert_id).unwrap())
}

/// Check if a domain matches a pattern (exact match or glob)
pub fn matches_domain(pattern: &str, domain: &str) -> bool {
    if domain == pattern {
        return true;
    }
    glob_match(pattern, domain)
}

/// Find a matching certificate config for a domain
pub fn find_matching_certificate<'a>(
    certs: &'a HashMap<String, CertificateConfig>,
    domain: &str,
) -> Option<&'a CertificateConfig> {
    let mut matches: Vec<_> = certs
        .values()
        .filter(|c| {
            c.names
                .as_ref()
                .map(|n| n.iter().any(|n| matches_domain(n, domain)))
                .unwrap_or(false)
                || c.domains
                    .as_ref()
                    .map(|d| d.iter().any(|d| matches_domain(d, domain)))
                    .unwrap_or(false)
        })
        .collect();

    // Prefer .names over .domains
    matches.sort_by_key(|c| if c.names.is_some() { 0 } else { 1 });
    matches.first().copied()
}

/// Delete certificates from DB that are no longer defined in config
pub fn cleanup_removed_certificates(
    db_pool: &DbPool,
    certificates: &HashMap<String, CertificateConfig>,
) {
    let mut conn = match db_pool.get() {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to get database connection for cleanup: {}", e);
            return;
        }
    };

    let all_certs = db::get_all_certificates(&mut conn);
    for cert in all_certs {
        // Delete if source certificate config no longer exists
        if !certificates.contains_key(&cert.source) {
            info!(
                "Deleting certificate {} (source config '{}' no longer exists)",
                cert.id, cert.source
            );
            db::delete_certificate(&mut conn, &cert.id);
        }
    }
}

/// Check and issue certificates on startup (runs in background)
pub fn start_startup_issuance(
    db_pool: DbPool,
    providers: HashMap<String, Arc<dyn Provider>>,
    certificates: HashMap<String, CertificateConfig>,
    challenge_store: SharedChallengeStore,
) {
    tokio::spawn(async move {
        check_and_issue_certificates(&db_pool, &providers, &certificates, &challenge_store).await;
    });
}

async fn check_and_issue_certificates(
    db_pool: &DbPool,
    providers: &HashMap<String, Arc<dyn Provider>>,
    certificates: &HashMap<String, CertificateConfig>,
    challenge_store: &SharedChallengeStore,
) {
    info!("Checking certificates on startup...");

    for (cert_id, cert_config) in certificates {
        // Only process certificates with explicit names (not dynamic domains)
        let Some(ref names) = cert_config.names else {
            continue;
        };

        let mut conn = match db_pool.get() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to get database connection: {}", e);
                continue;
            }
        };

        // Check if certificate needs issuance
        let existing = db::get_certificate(&mut conn, cert_id);
        let reason = check_needs_issuance(existing.as_ref(), cert_config, names);

        let Some(reason) = reason else {
            continue;
        };

        // Delete certificate if provider changed (need fresh start with new provider)
        if matches!(reason, IssuanceReason::ProviderChanged { .. }) {
            db::delete_certificate(&mut conn, cert_id);
        }

        // Get provider
        let Some(provider) = providers.get(&cert_config.provider) else {
            warn!(
                "Provider {} not found for certificate {}",
                cert_config.provider, cert_id
            );
            continue;
        };

        // Issue certificate
        info!("Certificate {} {}, issuing with provider {}", cert_id, reason, cert_config.provider);
        match issue_certificate(
            &mut conn,
            cert_id,
            cert_id,
            provider,
            &cert_config.provider,
            names,
            challenge_store.clone(),
            None,
        )
        .await
        {
            Ok(_) => {
                info!("Certificate {} issued successfully", cert_id);
            }
            Err(e) => {
                warn!("Failed to issue certificate {}: {}", cert_id, e);
            }
        }
    }

    info!("Startup certificate check complete");
}
