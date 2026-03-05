use acme_distributor_common::IssuanceResult;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::challenge_store::ChallengeStore;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Issuance failed: {0}")]
    IssuanceFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Certificate parsing error: {0}")]
    ParseError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type ChallengeStore_ = Arc<RwLock<ChallengeStore>>;

#[async_trait]
pub trait Provider: Send + Sync {
    /// Issue a certificate for the given domains
    async fn issue(
        &self,
        id: &str,
        names: &[String],
        challenge_store: ChallengeStore_,
    ) -> Result<IssuanceResult, ProviderError>;

    /// Provider identifier
    fn id(&self) -> &str;
}
