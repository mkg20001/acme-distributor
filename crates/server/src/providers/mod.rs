mod acme_lib;
mod acme_sh;
mod ca;
mod file;
mod traits;

pub use acme_lib::AcmeLibProvider;
pub use acme_sh::AcmeShProvider;
pub use ca::CaProvider;
pub use file::FileProvider;
pub use traits::*;

use acme_distributor_common::ProviderConfig;
use std::path::Path;
use std::sync::Arc;

pub fn create_provider(id: &str, config: &ProviderConfig, state_path: &Path) -> Arc<dyn Provider> {
    match config.provider_type.as_str() {
        "acme-sh" => Arc::new(AcmeShProvider::new(id, config, state_path)),
        "acme-lib" => Arc::new(AcmeLibProvider::new(id, config, state_path)),
        "ca" => Arc::new(CaProvider::new(id, config)),
        "file" => Arc::new(FileProvider::new(id, config, state_path)),
        other => panic!("Unknown provider type: {}", other),
    }
}
