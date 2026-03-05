use serde::{Deserialize, Serialize};
use std::path::Path;

const TWO_DAYS_MS: i64 = 2 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientState {
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

impl ClientState {
    pub fn load<P: AsRef<Path>>(path: P) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
    }

    pub fn should_renew(&self) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        let in_two_days = now + TWO_DAYS_MS;

        // Don't renew if preferRenewAfter hasn't been reached
        if let Some(after) = self.prefer_renew_after {
            if now < after {
                return false;
            }
        }

        // Must renew if expires in less than 2 days
        if self.expires_at < in_two_days {
            return true;
        }

        // Should renew if past preferRenewBefore
        if let Some(before) = self.prefer_renew_before {
            if now > before {
                return true;
            }
        }

        false
    }
}
