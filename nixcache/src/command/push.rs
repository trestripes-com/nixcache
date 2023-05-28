use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::path::PathBuf;
use clap::Parser;
use indicatif::MultiProgress;

use crate::api::ApiClient;
use crate::cli::Opts;
use crate::config::Config;
use attic::nix_store::NixStore;

/// Push closures to a binary cache.
#[derive(Debug, Parser)]
pub struct Push {
    /// The store paths to push.
    paths: Vec<PathBuf>,
    /// Push the specified paths only and do not compute closures.
    #[clap(long)]
    no_closure: bool,
    /// The maximum number of parallel upload processes.
    #[clap(short = 'j', long, default_value = "5")]
    jobs: usize,
}

pub async fn run(opts: Opts) -> Result<()> {
    let sub = opts.command.as_push().unwrap();
    if sub.jobs == 0 {
        return Err(anyhow!("The number of jobs cannot be 0"));
    }

    let config = Config::load()?;

    let store = Arc::new(NixStore::connect()?);
    let roots = sub
        .paths
        .clone()
        .into_iter()
        .map(|p| store.follow_store_path(p))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut api = ApiClient::from_server_config(config.server.clone())?;

    let push_config = PushConfig {
        num_workers: sub.jobs,
    };

    let mp = MultiProgress::new();
    let pusher = Pusher::new(store, api, mp, push_config);
    let plan = pusher
        .plan(roots, sub.no_closure)
        .await?;

    if plan.store_path_map.is_empty() {
        if plan.num_all_paths == 0 {
            eprintln!("🤷 Nothing selected.");
        } else {
            eprintln!(
                "✅ All done! ({num_already_cached} already cached, {num_upstream} in upstream)",
                num_already_cached = plan.num_already_cached,
                num_upstream = plan.num_upstream,
            );
        }

        return Ok(());
    } else {
        eprintln!("⚙️ Pushing {num_missing_paths} paths \"{server}\" ({num_already_cached} already cached, {num_upstream} in upstream)...",
            server = server_name.as_str(),
            num_missing_paths = plan.store_path_map.len(),
            num_already_cached = plan.num_already_cached,
            num_upstream = plan.num_upstream,
        );
    }

    for (_, path_info) in plan.store_path_map {
        pusher.queue(path_info).await?;
    }

    let results = pusher.wait().await;
    results.into_values().collect::<Result<Vec<()>>>()?;

    Ok(())
}
