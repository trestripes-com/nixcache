mod cli;
mod command;
mod config;
mod api;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    init_logging()?;
    cli::run().await
}

fn init_logging() -> Result<()> {
    tracing_subscriber::fmt::init();
    Ok(())
}