use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "acme-distributor-client")]
#[command(about = "ACME certificate distributor client")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Fetch and renew a certificate
    Fetch {
        /// Domain name to fetch certificate for
        #[arg(short, long)]
        domain: String,

        /// Path to state file (JSON)
        #[arg(short, long)]
        state: PathBuf,

        /// Path to credential file
        #[arg(short = 'c', long)]
        credential: PathBuf,

        /// Server URL
        #[arg(short = 'u', long)]
        server_url: String,

        /// Output path for certificate
        #[arg(long)]
        out_cert: PathBuf,

        /// Output path for private key
        #[arg(long)]
        out_key: PathBuf,

        /// Output path for CA certificate
        #[arg(long)]
        out_ca: PathBuf,

        /// Output path for full chain
        #[arg(long)]
        out_chain: PathBuf,

        /// Force renewal even if not needed
        #[arg(long, default_value = "false")]
        force: bool,
    },

    /// Check if renewal is needed (exit code 0 = needs renewal, 1 = no renewal needed)
    Check {
        /// Path to state file
        #[arg(short, long)]
        state: PathBuf,
    },
}
