use anyhow::Result;
use std::path::Path;
use std::net::SocketAddr;
use std::fs::read_to_string;
use serde::Deserialize;
use async_compression::Level as CompressionLevel;

use crate::storage::local::LocalStorageConfig;
use crate::narinfo::Compression as NixCompression;

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
    /// Compression.
    #[serde(default = "Default::default")]
    pub compression: CompressionConfig,
}

/// File storage configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum StorageConfig {
    /// Local file storage.
    #[serde(rename = "local")]
    Local(LocalStorageConfig),
}

/// Compression configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CompressionConfig {
    /// Compression type.
    pub r#type: CompressionType,

    /// Compression level.
    ///
    /// If unspecified, Attic will choose a default one.
    pub level: Option<u32>,
}
impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            r#type: CompressionType::Zstd,
            level: None,
        }
    }
}
impl CompressionConfig {
    pub fn level(&self) -> CompressionLevel {
        if let Some(level) = self.level {
            return CompressionLevel::Precise(level);
        }

        match self.r#type {
            CompressionType::Brotli => CompressionLevel::Precise(5),
            CompressionType::Zstd => CompressionLevel::Precise(8),
            CompressionType::Xz => CompressionLevel::Precise(2),
            _ => CompressionLevel::Default,
        }
    }
}
/// Compression type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum CompressionType {
    /// No compression.
    #[serde(rename = "none")]
    None,
    /// Brotli.
    #[serde(rename = "brotli")]
    Brotli,
    /// ZSTD.
    #[serde(rename = "zstd")]
    Zstd,
    /// XZ.
    #[serde(rename = "xz")]
    Xz,
}
impl From<CompressionType> for NixCompression {
    fn from(t: CompressionType) -> Self {
        match t {
            CompressionType::None => NixCompression::None,
            CompressionType::Brotli => NixCompression::Brotli,
            CompressionType::Zstd => NixCompression::Zstd,
            CompressionType::Xz => NixCompression::Xz,
        }
    }
}

fn default_listen_address() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}
