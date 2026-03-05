mod acme_sh;
mod ca;
mod traits;

pub use acme_sh::AcmeShProvider;
pub use ca::CaProvider;
pub use traits::*;

use acme_distributor_common::ProviderConfig;
use std::path::Path;
use std::sync::Arc;

pub fn create_provider(
    id: &str,
    config: &ProviderConfig,
    state_path: &Path,
) -> Arc<dyn Provider> {
    match config.provider_type.as_str() {
        "acme-sh" => Arc::new(AcmeShProvider::new(id, config, state_path)),
        "ca" => Arc::new(CaProvider::new(id, config)),
        other => panic!("Unknown provider type: {}", other),
    }
}
