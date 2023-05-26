use anyhow::Result;
use std::path::Path;
use std::net::SocketAddr;
use std::fs::read_to_string;
use serde::Deserialize;

use crate::storage::local::LocalStorageConfig;

const CONFIG_PATH: &str = "/trestripes/nixcache/config.toml";
const LOCAL_CONFIG_PATH: &str = "./config.toml";

pub async fn load() -> Result<Config> {
    let data = if Path::new(LOCAL_CONFIG_PATH).is_file() {
        read_to_string(Path::new(LOCAL_CONFIG_PATH)).unwrap()
    } else {
        read_to_string(Path::new(CONFIG_PATH)).unwrap()
    };

    let config = toml::from_str(&data)?;

    Ok(config)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Socket address to listen on.
    #[serde(default = "default_listen_address")]
    pub listen: SocketAddr,
    /// Storage.
    pub storage: StorageConfig,
}

/// File storage configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum StorageConfig {
    /// Local file storage.
    #[serde(rename = "local")]
    Local(LocalStorageConfig),
}

fn default_listen_address() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}
