mod certificate;
mod challenge;

pub use certificate::*;
pub use challenge::*;

use rocket::get;
use rocket::http::ContentType;

/// GET / - Status endpoint
#[get("/")]
pub fn status() -> (ContentType, &'static str) {
    (ContentType::Plain, "ACME distributor running!")
}
