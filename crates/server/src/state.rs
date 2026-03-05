use acme_distributor_common::{CertificateConfig, Config, TokenConfig};
use std::collections::HashMap;
use std::sync::Arc;

use crate::challenge_store::SharedChallengeStore;
use crate::db::DbPool;
use crate::providers::Provider;

pub struct AppState {
    pub db_pool: DbPool,
    pub challenge_store: SharedChallengeStore,
    pub providers: HashMap<String, Arc<dyn Provider>>,
    pub config: Config,
    pub tokens: HashMap<String, TokenConfig>,
    pub certificates: HashMap<String, CertificateConfig>,
}
