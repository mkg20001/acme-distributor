use acme_distributor_common::TokenConfig;
use glob_match::glob_match;
use ipnet::IpNet;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use sha2::{Digest, Sha512};
use std::collections::HashMap;
use std::net::IpAddr;
use tracing::debug;

use crate::state::AppState;

#[derive(Debug)]
pub struct AuthenticatedRequest {
    pub token_config: TokenConfig,
    pub client_ip: IpAddr,
}

#[derive(Debug)]
pub enum AuthError {
    MissingCredential,
    InvalidToken,
    IpNotAllowed,
}

fn extract_client_ip(req: &Request<'_>) -> IpAddr {
    // Try X-Forwarded-For first
    if let Some(xff) = req.headers().get_one("x-forwarded-for") {
        if let Some(first_ip) = xff.split(',').next() {
            if let Ok(ip) = first_ip.trim().parse() {
                return ip;
            }
        }
    }

    // Fall back to client IP from socket
    req.client_ip()
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap())
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn ip_matches_cidr(client: IpAddr, cidr: &str) -> bool {
    if cidr.contains('/') {
        if let Ok(net) = cidr.parse::<IpNet>() {
            return net.contains(&client);
        }
    } else if let Ok(allowed_ip) = cidr.parse::<IpAddr>() {
        return allowed_ip == client;
    }
    false
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthenticatedRequest {
    type Error = AuthError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // Extract x-credential header
        let credential = match req.headers().get_one("x-credential") {
            Some(c) => c,
            None => {
                debug!("Missing x-credential header");
                return Outcome::Error((Status::Unauthorized, AuthError::MissingCredential));
            }
        };

        // Hash the credential
        let hashed = hash_token(credential);

        // Look up token in state
        let state = req
            .rocket()
            .state::<AppState>()
            .expect("AppState not configured");

        let token_config = match state.tokens.get(&hashed) {
            Some(t) => t.clone(),
            None => {
                debug!("Invalid token: {}", &hashed[..16]);
                return Outcome::Error((Status::Unauthorized, AuthError::InvalidToken));
            }
        };

        // Extract client IP
        let client_ip = extract_client_ip(req);
        debug!("Client IP: {}", client_ip);

        // Validate IP against allowFrom CIDRs
        if let Some(ref allow_from) = token_config.allow_from {
            let ip_allowed = allow_from
                .iter()
                .any(|cidr| ip_matches_cidr(client_ip, cidr));
            if !ip_allowed {
                debug!("IP {} not in allowed list: {:?}", client_ip, allow_from);
                return Outcome::Error((Status::Unauthorized, AuthError::IpNotAllowed));
            }
        }

        Outcome::Success(AuthenticatedRequest {
            token_config,
            client_ip,
        })
    }
}

impl AuthenticatedRequest {
    pub fn can_access_domain(&self, domain: &str) -> bool {
        match &self.token_config.restrict_names {
            None => true,
            Some(patterns) => patterns
                .iter()
                .any(|pattern| domain == pattern || glob_match(pattern, domain)),
        }
    }
}

/// Build tokens map from config (hashed -> config)
pub fn build_tokens_map(tokens: &[TokenConfig]) -> HashMap<String, TokenConfig> {
    let mut map = HashMap::new();
    for token in tokens {
        let hashed = if let Some(ref h) = token.hashed {
            h.clone()
        } else if let Some(ref p) = token.plain {
            hash_token(p)
        } else {
            continue;
        };
        map.insert(hashed, token.clone());
    }
    map
}
