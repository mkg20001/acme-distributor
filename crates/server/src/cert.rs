use acme_distributor_common::CertificateConfig;
use diesel::SqliteConnection;
use glob_match::glob_match;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::challenge_store::SharedChallengeStore;
use crate::db::{self, DbPool, NewCertificate};
use crate::providers::{Provider, ProviderError};

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

        // Check if certificate exists and is valid
        let existing = db::get_certificate(&mut conn, cert_id);
        let needs_issuance = match existing {
            None => {
                info!("Certificate {} not found, will issue", cert_id);
                true
            }
            Some(ref cert) if cert.is_expired() => {
                info!("Certificate {} is expired, will issue", cert_id);
                true
            }
            Some(_) => false,
        };

        if !needs_issuance {
            continue;
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
        info!(
            "Issuing certificate {} with provider {}",
            cert_id, cert_config.provider
        );
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
