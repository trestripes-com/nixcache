use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::fs::{self, OpenOptions, Permissions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::io::Write;
use serde::{Serialize, Deserialize};
use xdg::BaseDirectories;

/// Application prefix in XDG base directories.
///
/// This will be concatenated into `$XDG_CONFIG_HOME/nixcache`.
const XDG_PREFIX: &str = "nixcache";

/// Config filename.
const CONFIG_FILENAME: &str = "config.toml";

/// The permission the configuration file should have.
const FILE_MODE: u32 = 0o600;

pub struct Config {
    pub data: ConfigData,
    pub path: PathBuf,
}
impl Config {
    pub fn new(path: Option<PathBuf>, data: ConfigDataVersioned) -> Result<Self> {
        let path = match path {
            Some(path) => path,
            None => get_config_path()?,
        };

        Ok(Self {
            path,
            data: data.into(),
        })
    }

    /// Loads the configuration from the system.
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = match path {
            Some(path) => path,
            None => get_config_path()?,
        };

        if path.exists() {
            let contents = fs::read(&path)?;
            let s = std::str::from_utf8(&contents)?;
            let data: ConfigDataVersioned = toml::from_str(s)?;
            return Ok(Config {
                path,
                data: data.into(),
            });
        }

        Err(anyhow!("No config found at '{}'.", path.to_string_lossy()))
    }
    /// Saves the configuration back to the system, if possible.
    pub fn save(&self) -> Result<()> {
        let config: ConfigDataVersioned = self.data.clone().into();
        let serialized = toml::to_string(&config)?;

        // This isn't atomic, so some other process might chmod it
        // to something else before we write. We don't handle this case.
        if self.path.exists() {
            let permissions = Permissions::from_mode(FILE_MODE);
            fs::set_permissions(&self.path, permissions)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .mode(FILE_MODE)
            .open(&self.path)?;

        file.write_all(serialized.as_bytes())?;

        tracing::debug!("Saved modified configuration to {:?}", self.path);

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum ConfigDataVersioned {
    #[serde(rename = "v1")]
    V1(ConfigData),
}
impl Into<ConfigData> for ConfigDataVersioned {
    fn into(self) -> ConfigData {
        match self {
            ConfigDataVersioned::V1(config) => config,
        }
    }
}
impl From<ConfigData> for ConfigDataVersioned {
    fn from(config: ConfigData) -> Self {
        ConfigDataVersioned::V1(config)
    }
}

/// Client configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigData {
    /// The server to connect to.
    pub server: ServerConfig,
}

/// Configuration of a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub endpoint: String,
    pub token: Option<String>,
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8080".to_string(),
            token: None,
        }
    }
}

fn get_config_path() -> Result<PathBuf> {
    let xdg_dirs = BaseDirectories::with_prefix(XDG_PREFIX)?;
    let config_path = xdg_dirs.place_config_file(CONFIG_FILENAME)?;
    Ok(config_path)
}
