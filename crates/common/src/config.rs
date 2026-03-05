use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub state: String,
    pub providers: HashMap<String, ProviderConfig>,
    pub certificates: HashMap<String, CertificateConfig>,
    pub tokens: Vec<TokenConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(rename = "trust-proxy")]
    pub trust_proxy: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub dns: Option<String>,
    pub env: Option<HashMap<String, String>>,
    // For CA provider
    pub key: Option<String>,
    pub cert: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CertificateConfig {
    #[serde(skip)]
    pub id: String,
    pub names: Option<Vec<String>>,
    pub domains: Option<Vec<String>>,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenConfig {
    pub hashed: Option<String>,
    pub plain: Option<String>,
    #[serde(rename = "allowFrom")]
    pub allow_from: Option<Vec<String>>,
    #[serde(rename = "restrictNames")]
    pub restrict_names: Option<Vec<String>>,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, crate::ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = serde_yaml::from_str(&content)?;

        // Populate certificate IDs from map keys
        for (id, cert) in &mut config.certificates {
            cert.id = id.clone();
        }

        Ok(config)
    }
}
