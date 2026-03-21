use acme_distributor_common::CertificateResponse;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, State};
use tracing::{debug, info};

use crate::cert::{find_matching_certificate, issue_certificate_locked};
use crate::db;
use crate::guards::AuthenticatedRequest;
use crate::state::AppState;

/// GET /certificate/<name>
#[get("/certificate/<name>")]
pub async fn get_certificate(
    name: &str,
    auth: AuthenticatedRequest,
    state: &State<AppState>,
) -> Result<Json<CertificateResponse>, Status> {
    // Validate domain access
    if !auth.can_access_domain(name) {
        debug!("Token not allowed to access domain: {}", name);
        return Err(Status::NotFound);
    }

    // Find matching certificate config
    let cert_config = find_matching_certificate(&state.certificates, name).ok_or_else(|| {
        debug!("No matching certificate config for: {}", name);
        Status::NotFound
    })?;

    // Determine certificate ID
    let cert_id = if cert_config.names.is_some() {
        cert_config.id.clone()
    } else {
        format!("{}#{}", cert_config.id, name)
    };

    // Check database for existing certificate
    let mut conn = state
        .db_pool
        .get()
        .map_err(|_| Status::InternalServerError)?;

    let existing = db::get_certificate(&mut conn, &cert_id);

    // Issue if expired or missing
    let cert = match existing {
        Some(c) if !c.is_expired() => {
            debug!("Using existing certificate: {}", cert_id);
            c
        }
        _ => {
            info!("Issuing new certificate: {}", cert_id);
            let names = cert_config
                .names
                .clone()
                .unwrap_or_else(|| vec![name.to_string()]);

            let provider = state.providers.get(&cert_config.provider).ok_or_else(|| {
                debug!("Provider not found: {}", cert_config.provider);
                Status::InternalServerError
            })?;

            issue_certificate_locked(
                &state.db_pool,
                &cert_id,
                &cert_config.id,
                provider,
                &cert_config.provider,
                &names,
                state.challenge_store.clone(),
                None,
                &state.renewal_locks,
            )
            .await
            .map_err(|e| {
                tracing::error!("Issuance failed: {}", e);
                Status::InternalServerError
            })?
        }
    };

    // Update requested_at timestamp
    db::update_requested_at(&mut conn, &cert_id);

    Ok(Json(CertificateResponse {
        cert: cert.cert_pem,
        key: cert.key_pem,
        ca: cert.ca_pem,
        chain: cert.chain_pem,
        expires_at: cert.expires_at,
        prefer_renew_before: cert.prefer_renew_before,
        prefer_renew_after: cert.prefer_renew_after,
    }))
}
