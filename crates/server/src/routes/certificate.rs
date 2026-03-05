use acme_distributor_common::{CertificateConfig, CertificateResponse};
use glob_match::glob_match;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, State};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::db::{self, NewCertificate};
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

            let result = provider
                .issue(&cert_id, &names, state.challenge_store.clone())
                .await
                .map_err(|e| {
                    tracing::error!("Issuance failed: {}", e);
                    Status::InternalServerError
                })?;

            let new_cert = NewCertificate {
                id: cert_id.clone(),
                source: cert_config.id.clone(),
                provider: cert_config.provider.clone(),
                names: serde_json::to_string(&names).unwrap(),
                cert_pem: Some(result.cert_pem),
                key_pem: Some(result.key_pem),
                ca_pem: Some(result.ca_pem),
                chain_pem: Some(result.chain_pem),
                expires_at: result.expires_at,
                prefer_renew_before: result.prefer_renew_before,
                prefer_renew_after: None,
                requested_at: chrono::Utc::now().timestamp_millis(),
            };

            db::upsert_certificate(&mut conn, new_cert);
            db::get_certificate(&mut conn, &cert_id).unwrap()
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

fn matches_domain(pattern: &str, domain: &str) -> bool {
    if domain == pattern {
        return true;
    }
    glob_match(pattern, domain)
}

fn find_matching_certificate<'a>(
    certs: &'a HashMap<String, CertificateConfig>,
    domain: &str,
) -> Option<&'a CertificateConfig> {
    let mut matches: Vec<_> = certs
        .values()
        .filter(|c| {
            c.names
                .as_ref()
                .map(|n| n.iter().any(|n| matches_domain(n, domain)))
                .unwrap_or(false)
                || c.domains
                    .as_ref()
                    .map(|d| d.iter().any(|d| matches_domain(d, domain)))
                    .unwrap_or(false)
        })
        .collect();

    // Prefer .names over .domains
    matches.sort_by_key(|c| if c.names.is_some() { 0 } else { 1 });
    matches.first().copied()
}
