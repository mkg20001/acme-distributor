use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use super::schema::certificates;

#[derive(Queryable, Selectable, Identifiable, Debug, Clone, Serialize, Deserialize)]
#[diesel(table_name = certificates)]
pub struct Certificate {
    pub id: String,
    pub source: String,
    pub provider: String,
    pub names: String, // JSON serialized Vec<String>
    pub cert_pem: Option<String>,
    pub key_pem: Option<String>,
    pub ca_pem: Option<String>,
    pub chain_pem: Option<String>,
    pub expires_at: i64,
    pub prefer_renew_before: Option<i64>,
    pub prefer_renew_after: Option<i64>,
    pub requested_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Insertable, AsChangeset, Debug)]
#[diesel(table_name = certificates)]
pub struct NewCertificate {
    pub id: String,
    pub source: String,
    pub provider: String,
    pub names: String,
    pub cert_pem: Option<String>,
    pub key_pem: Option<String>,
    pub ca_pem: Option<String>,
    pub chain_pem: Option<String>,
    pub expires_at: i64,
    pub prefer_renew_before: Option<i64>,
    pub prefer_renew_after: Option<i64>,
    pub requested_at: i64,
}

impl Certificate {
    pub fn get_names(&self) -> Vec<String> {
        serde_json::from_str(&self.names).unwrap_or_default()
    }

    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        self.expires_at <= now
    }
}
