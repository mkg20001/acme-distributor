use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse config: {0}")]
    Parse(#[from] serde_yaml::Error),
}

#[derive(Error, Debug)]
pub enum AcmeError {
    #[error("Certificate not found")]
    NotFound,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("IP not allowed")]
    IpNotAllowed,
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
