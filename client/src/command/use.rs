use anyhow::Result;
use clap::Parser;
use reqwest::Url;

use crate::api::Client;
use crate::cli::Opts;
use crate::config::Config;
use crate::nix_config::NixConfig;
use crate::nix_netrc::NixNetrc;

/// Configure Nix to use a binary cache.
#[derive(Debug, Parser)]
pub struct Use;

pub async fn run(opts: Opts) -> Result<()> {
    let _sub = opts.command.as_use().unwrap();
    let config = Config::load(opts.config)?;

    let server = config.data.server;

    let api = Client::from_server_config(server.clone())?;
    let cache_config = api.get_cache_config().await?;

    let substituter = cache_config.substituter_endpoint.unwrap_or(server.endpoint.clone());
    let public_key = cache_config.public_key.unwrap_or(server.endpoint.clone());

    eprintln!("Configuring Nix to use \"{}\":", server.endpoint);

    // Modify nix.conf
    eprintln!("+ Substituter: {}", substituter);
    eprintln!("+ Trusted Public Key: {}", public_key);

    let mut nix_config = NixConfig::load().await?;
    nix_config.add_substituter(&substituter);
    nix_config.add_trusted_public_key(&public_key);

    // Modify netrc
    if let Some(token) = &server.token {
        eprintln!("+ Access Token");

        let mut nix_netrc = NixNetrc::load().await?;
        let host = Url::parse(&substituter)?
            .host()
            .map(|h| h.to_string())
            .unwrap();
        nix_netrc.add_token(host, token.to_string());
        nix_netrc.save().await?;

        let netrc_path = nix_netrc.path().unwrap().to_str().unwrap();

        nix_config.set_netrc_file(netrc_path);
    }

    nix_config.save().await?;

    Ok(())
}
