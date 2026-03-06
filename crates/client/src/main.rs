use acme_distributor_common::CertificateResponse;
use anyhow::{Context, Result};
use clap::Parser;
use std::path::Path;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod cli;
mod state;

use cli::{Cli, Commands};
use state::ClientState;

async fn fetch_certificate(
    server_url: &str,
    domain: &str,
    credential: &str,
) -> Result<CertificateResponse> {
    let url = format!(
        "{}/certificate/{}",
        server_url.trim_end_matches('/'),
        domain
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("X-Credential", credential)
        .send()
        .await
        .context("Failed to send request")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Server returned error: {} {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("Unknown")
        );
    }

    response.json().await.context("Failed to parse response")
}

fn write_if_some<P: AsRef<Path>>(path: P, content: &Option<String>) -> Result<()> {
    if let Some(c) = content {
        std::fs::write(&path, c)
            .with_context(|| format!("Failed to write {}", path.as_ref().display()))?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Fetch {
            domain,
            state,
            credential,
            server_url,
            out_cert,
            out_key,
            out_ca,
            out_chain,
            force,
        } => {
            info!("acme-distributor-client starting");

            // Check if renewal is needed
            let needs_renewal = if force {
                info!("Forced renewal requested");
                true
            } else if let Some(current_state) = ClientState::load(&state) {
                if current_state.should_renew() {
                    info!("Certificate needs renewal");
                    true
                } else {
                    info!("No renewal necessary, certificate still valid");
                    false
                }
            } else {
                info!("No existing certificate, must fetch");
                true
            };

            if !needs_renewal {
                return Ok(());
            }

            // Read credential
            let cred = std::fs::read_to_string(&credential)
                .with_context(|| {
                    format!("Failed to read credential file: {}", credential.display())
                })?
                .trim()
                .to_string();

            // Fetch certificate
            info!("Fetching certificate for {}", domain);
            let response = fetch_certificate(&server_url, &domain, &cred).await?;

            // Save state
            let new_state = ClientState {
                cert: response.cert.clone(),
                key: response.key.clone(),
                ca: response.ca.clone(),
                chain: response.chain.clone(),
                expires_at: response.expires_at,
                prefer_renew_before: response.prefer_renew_before,
                prefer_renew_after: response.prefer_renew_after,
            };
            new_state
                .save(&state)
                .context("Failed to save state file")?;

            // Write certificate files
            info!("Writing certificate files");
            write_if_some(&out_cert, &response.cert)?;
            write_if_some(&out_key, &response.key)?;
            write_if_some(&out_ca, &response.ca)?;
            write_if_some(&out_chain, &response.chain)?;

            info!("Done!");
        }

        Commands::Check { state } => {
            if let Some(current_state) = ClientState::load(&state) {
                if current_state.should_renew() {
                    // Exit 0 = needs renewal
                    std::process::exit(0);
                } else {
                    // Exit 1 = no renewal needed
                    std::process::exit(1);
                }
            } else {
                // No state file = needs renewal
                std::process::exit(0);
            }
        }
    }

    Ok(())
}
