use rocket::http::Status;
use rocket::{get, State};

use crate::challenge_store::SharedChallengeStore;

/// GET /.well-known/acme-challenge/<challenge>
#[get("/.well-known/acme-challenge/<challenge>")]
pub async fn serve_challenge(
    challenge: &str,
    challenge_store: &State<SharedChallengeStore>,
) -> Result<String, Status> {
    let store = challenge_store.read().await;
    store.get(challenge).ok_or(Status::NotFound)
}
