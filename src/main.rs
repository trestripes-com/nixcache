use anyhow::Result;

use nixcache::{run_api_server, config};

#[tokio::main]
async fn main() -> Result<()> {
    dump_version();

    let config = config::load().await?;

    run_api_server(opts.listen, config).await?;

    Ok(())
}

fn dump_version() {
    #[cfg(debug_assertions)]
    eprintln!("Nixcache {} (debug)", env!("CARGO_PKG_VERSION"));
    #[cfg(not(debug_assertions))]
    eprintln!("Nixcache {} (release)", env!("CARGO_PKG_VERSION"));
}
