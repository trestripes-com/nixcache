use anyhow::Result;
use std::path::PathBuf;
use std::fs;
use serde::Deserialize;
use xdg::BaseDirectories;

/// Application prefix in XDG base directories.
///
/// This will be concatenated into `$XDG_CONFIG_HOME/nixcache`.
const XDG_PREFIX: &str = "nixcache";

/// Client configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    /// The server to connect to.
    #[serde(default = "Default::default")]
    pub server: ServerConfig,
}

/// Configuration of a server.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub endpoint: String,
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self { endpoint: "http://localhost:8080".to_string() }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = get_config_path()?;
        if path.exists() {
            let contents = fs::read(path)?;
            let s = std::str::from_utf8(&contents)?;
            let config = toml::from_str(s)?;
            return Ok(config);
        }
        Ok(Config::default())
    }
}

fn get_config_path() -> Result<PathBuf> {
    let xdg_dirs = BaseDirectories::with_prefix(XDG_PREFIX)?;
    let config_path = xdg_dirs.place_config_file("config.toml")?;
    Ok(config_path)
}
