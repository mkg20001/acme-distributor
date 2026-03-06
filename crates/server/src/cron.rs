use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::cert::issue_certificate;
use crate::challenge_store::SharedChallengeStore;
use crate::db::{self, DbPool};
use crate::providers::Provider;
use acme_distributor_common::CertificateConfig;

const CRON_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1 hour
const SIXTY_DAYS_MS: i64 = 60 * 24 * 60 * 60 * 1000;
const TWO_DAYS_MS: i64 = 2 * 24 * 60 * 60 * 1000;

pub fn start_cron(
    db_pool: DbPool,
    providers: HashMap<String, Arc<dyn Provider>>,
    certificates: HashMap<String, CertificateConfig>,
    challenge_store: SharedChallengeStore,
) {
    tokio::spawn(async move {
        let mut interval = interval(CRON_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(e) = run_cron(&db_pool, &providers, &certificates, &challenge_store).await {
                warn!("Cron job failed: {}", e);
            }
        }
    });
}

async fn run_cron(
    db_pool: &DbPool,
    providers: &HashMap<String, Arc<dyn Provider>>,
    certificates: &HashMap<String, CertificateConfig>,
    challenge_store: &SharedChallengeStore,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Running certificate maintenance cron");

    let mut conn = db_pool.get()?;
    let certs = db::get_all_certificates(&mut conn);

    let now = chrono::Utc::now().timestamp_millis();
    let sixty_days_ago = now - SIXTY_DAYS_MS;
    let in_two_days = now + TWO_DAYS_MS;

    for cert in certs {
        // Delete if not requested in 60+ days and not a named certificate
        if cert.requested_at < sixty_days_ago && !certificates.contains_key(&cert.id) {
            info!("Deleting stale certificate: {}", cert.id);
            db::delete_certificate(&mut conn, &cert.id);
            continue;
        }

        // Delete if provider doesn't exist
        if !providers.contains_key(&cert.provider) {
            info!(
                "Deleting certificate with missing provider: {} (provider: {})",
                cert.id, cert.provider
            );
            db::delete_certificate(&mut conn, &cert.id);
            continue;
        }

        // Delete if source certificate config doesn't exist
        if !certificates.contains_key(&cert.source) {
            info!(
                "Deleting certificate with missing source: {} (source: {})",
                cert.id, cert.source
            );
            db::delete_certificate(&mut conn, &cert.id);
            continue;
        }

        // Check if renewal is needed
        let should_renew = {
            let past_renew_after = cert.prefer_renew_after.map(|t| t < now).unwrap_or(true);
            let past_renew_before = cert.prefer_renew_before.map(|t| t < now).unwrap_or(false);
            let expires_soon = cert.expires_at < in_two_days;

            past_renew_after && (past_renew_before || expires_soon)
        };

        if should_renew {
            info!("Renewing certificate: {}", cert.id);

            let provider = providers.get(&cert.provider).unwrap();
            let names = cert.get_names();

            match issue_certificate(
                &mut conn,
                &cert.id,
                &cert.source,
                provider,
                &cert.provider,
                &names,
                challenge_store.clone(),
                Some(cert.requested_at),
            )
            .await
            {
                Ok(_) => {
                    info!("Certificate {} renewed successfully", cert.id);
                }
                Err(e) => {
                    warn!("Failed to renew certificate {}: {}", cert.id, e);
                }
            }
        }
    }

    // Cleanup expired challenges
    debug!("Cleaning up expired challenges");
    challenge_store.write().await.cleanup();

    info!("Cron job completed");
    Ok(())
}
