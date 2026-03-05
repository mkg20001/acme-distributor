use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const CHALLENGE_TTL: Duration = Duration::from_secs(10 * 60); // 10 minutes

pub struct ChallengeStore {
    challenges: HashMap<String, (String, Instant)>,
}

impl ChallengeStore {
    pub fn new() -> Self {
        Self {
            challenges: HashMap::new(),
        }
    }

    pub fn insert(&mut self, path: String, value: String) {
        self.challenges.insert(path, (value, Instant::now()));
    }

    pub fn get(&self, path: &str) -> Option<String> {
        self.challenges.get(path).and_then(|(value, created)| {
            if created.elapsed() < CHALLENGE_TTL {
                Some(value.clone())
            } else {
                None
            }
        })
    }

    pub fn cleanup(&mut self) {
        self.challenges
            .retain(|_, (_, created)| created.elapsed() < CHALLENGE_TTL);
    }
}

impl Default for ChallengeStore {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedChallengeStore = Arc<RwLock<ChallengeStore>>;

pub fn create_challenge_store() -> SharedChallengeStore {
    Arc::new(RwLock::new(ChallengeStore::new()))
}
