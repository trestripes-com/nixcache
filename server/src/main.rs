use anyhow::Result;
use std::path::PathBuf;
use clap::Parser;

use server::{run_api_server, config};

/// Nixcached - nixcache server.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the 'config.toml'.
    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    dump_version();
    tracing_subscriber::fmt::init();

    let config = config::load(args.config).await?;

    run_api_server(config).await?;

    Ok(())
}

fn dump_version() {
    #[cfg(debug_assertions)]
    eprintln!("Nixcache {} (debug)", env!("CARGO_PKG_VERSION"));
    #[cfg(not(debug_assertions))]
    eprintln!("Nixcache {} (release)", env!("CARGO_PKG_VERSION"));
}
