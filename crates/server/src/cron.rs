use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::cert::{check_needs_deletion, issue_certificate};
use crate::challenge_store::SharedChallengeStore;
use crate::db::{self, DbPool};
use crate::providers::Provider;
use acme_distributor_common::CertificateConfig;

const CRON_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1 hour

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

    for cert in certs {
        // Check if certificate should be deleted
        if let Some(reason) = check_needs_deletion(&cert, providers, certificates) {
            info!("Deleting certificate {}: {}", cert.id, reason);
            db::delete_certificate(&mut conn, &cert.id);
            continue;
        }

        // Check if renewal is needed
        if !cert.needs_renewal() {
            continue;
        }

        info!("Renewing certificate {} (in renewal window)", cert.id);

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

    // Cleanup expired challenges
    debug!("Cleaning up expired challenges");
    challenge_store.write().await.cleanup();

    info!("Cron job completed");
    Ok(())
}
