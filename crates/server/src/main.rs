#[macro_use]
extern crate rocket;

use acme_distributor_common::{CertificateConfig, Config};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use rocket::figment::Figment;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use challenge_store::SharedChallengeStore;
use providers::Provider;

mod challenge_store;
mod cron;
mod db;
mod guards;
mod providers;
mod routes;
mod state;

use challenge_store::create_challenge_store;
use db::{create_pool, DbPool};
use guards::build_tokens_map;
use providers::create_provider;
use state::AppState;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("../../migrations");

fn run_migrations(pool: &DbPool) {
    let mut conn = pool.get().expect("Failed to get database connection");
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");
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
        info!("Issuing certificate {} with provider {}", cert_id, cert_config.provider);
        match provider.issue(cert_id, names, challenge_store.clone()).await {
            Ok(result) => {
                let new_cert = db::NewCertificate {
                    id: cert_id.clone(),
                    source: cert_id.clone(),
                    provider: cert_config.provider.clone(),
                    names: serde_json::to_string(names).unwrap(),
                    cert_pem: Some(result.cert_pem),
                    key_pem: Some(result.key_pem),
                    ca_pem: Some(result.ca_pem),
                    chain_pem: Some(result.chain_pem),
                    expires_at: result.expires_at,
                    prefer_renew_before: result.prefer_renew_before,
                    prefer_renew_after: None,
                    requested_at: chrono::Utc::now().timestamp_millis(),
                };
                db::upsert_certificate(&mut conn, new_cert);
                info!("Certificate {} issued successfully", cert_id);
            }
            Err(e) => {
                warn!("Failed to issue certificate {}: {}", cert_id, e);
            }
        }
    }

    info!("Startup certificate check complete");
}

#[launch]
async fn rocket() -> _ {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Load configuration
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "config.yaml".to_string());

    info!("Loading config from: {}", config_path);
    let config = Config::load(&config_path).expect("Failed to load config");

    // Create state directory
    std::fs::create_dir_all(&config.state).expect("Failed to create state directory");

    // Initialize database
    let db_path = PathBuf::from(&config.state).join("db.sqlite");
    let database_url = format!("sqlite://{}", db_path.display());
    info!("Database: {}", database_url);

    let db_pool = create_pool(&database_url);
    run_migrations(&db_pool);

    // Create challenge store
    let challenge_store = create_challenge_store();

    // Initialize providers
    let state_path = PathBuf::from(&config.state);
    let mut providers_map: HashMap<String, Arc<dyn providers::Provider>> = HashMap::new();
    for (id, provider_config) in &config.providers {
        info!("Initializing provider: {} (type: {})", id, provider_config.provider_type);
        let provider = create_provider(id, provider_config, &state_path);
        providers_map.insert(id.clone(), provider);
    }

    // Build tokens map
    let tokens_map = build_tokens_map(&config.tokens);
    info!("Loaded {} tokens", tokens_map.len());

    // Clone certificates with IDs populated
    let certificates_map: HashMap<String, _> = config
        .certificates
        .iter()
        .map(|(id, c)| {
            let mut cert = c.clone();
            cert.id = id.clone();
            (id.clone(), cert)
        })
        .collect();
    info!("Loaded {} certificate configs", certificates_map.len());

    // Check and issue certificates on startup
    check_and_issue_certificates(
        &db_pool,
        &providers_map,
        &certificates_map,
        &challenge_store,
    )
    .await;

    // Start cron job
    cron::start_cron(
        db_pool.clone(),
        providers_map.clone(),
        certificates_map.clone(),
        challenge_store.clone(),
    );

    // Build app state
    let app_state = AppState {
        db_pool,
        challenge_store: challenge_store.clone(),
        providers: providers_map,
        config: config.clone(),
        tokens: tokens_map,
        certificates: certificates_map,
    };

    // Configure Rocket
    let figment = Figment::from(rocket::Config::default())
        .merge(("address", &config.server.host))
        .merge(("port", config.server.port));

    info!(
        "Starting server on {}:{}",
        config.server.host, config.server.port
    );

    rocket::custom(figment)
        .manage(app_state)
        .manage(challenge_store)
        .mount(
            "/",
            routes![routes::serve_challenge, routes::get_certificate],
        )
}
