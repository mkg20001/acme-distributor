#[macro_use]
extern crate rocket;

use acme_distributor_common::Config;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use rocket::figment::Figment;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod cert;
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
        info!(
            "Initializing provider: {} (type: {})",
            id, provider_config.provider_type
        );
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

    // Cleanup certificates no longer in config
    cert::cleanup_removed_certificates(&db_pool, &certificates_map);

    // Start background certificate issuance check
    cert::start_startup_issuance(
        db_pool.clone(),
        providers_map.clone(),
        certificates_map.clone(),
        challenge_store.clone(),
    );

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
            routes![routes::status, routes::serve_challenge, routes::get_certificate],
        )
}
