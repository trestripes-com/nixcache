use anyhow::Result;
use clap::Parser;

use crate::cli::Opts;
use crate::config::{ServerConfig, ConfigData, Config};

/// Init cache endpoint config.
#[derive(Debug, Clone, Parser)]
pub struct Init {
    /// Cache endpoint url.
    #[clap(short, long)]
    url: String,
}
impl Into<ConfigData> for Init {
    fn into(self) -> ConfigData {
        let server = ServerConfig {
            endpoint: self.url,
        };

        ConfigData {
            server,
        }
    }
}

pub async fn run(opts: Opts) -> Result<()> {
    let sub: &Init = opts.command.as_init().unwrap();

    let path = opts.config;
    let data: ConfigData = sub.clone().into();

    let config = Config::new(path, data)?;
    config.save()?;

    Ok(())
}
