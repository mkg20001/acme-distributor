use serde::{Deserialize, Serialize};

/// Response returned by the certificate endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub ca: Option<String>,
    pub chain: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    #[serde(rename = "preferRenewBefore")]
    pub prefer_renew_before: Option<i64>,
    #[serde(rename = "preferRenewAfter")]
    pub prefer_renew_after: Option<i64>,
}

/// Result of a certificate issuance
#[derive(Debug, Clone)]
pub struct IssuanceResult {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
    pub chain_pem: String,
    pub expires_at: i64,
    pub prefer_renew_before: Option<i64>,
}
