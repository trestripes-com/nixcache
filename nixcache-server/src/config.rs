use anyhow::Result;
use std::path::Path;
use std::net::SocketAddr;
use std::fs::read_to_string;
use serde::{Serialize, Deserialize};
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
    /// Data chunking.
    pub chunking: ChunkingConfig,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            return CompressionLevel::Precise(level.try_into().unwrap());
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
/// Data chunking.
///
/// This must be set, but a default set of values is provided
/// through the OOBE sequence. The reason is that this allows
/// us to provide a new set of recommended "defaults" for newer
/// deployments without affecting existing ones.
///
/// Warning: If you change any of the values here, it will be
/// difficult to reuse existing chunks for newly-uploaded NARs
/// since the cutpoints will be different. As a result, the
/// deduplication ratio will suffer for a while after the change.
#[derive(Debug, Clone, Deserialize)]
pub struct ChunkingConfig {
    /// The minimum NAR size to trigger chunking.
    ///
    /// If 0, chunking is disabled entirely for newly-uploaded
    /// NARs.
    ///
    /// If 1, all newly-uploaded NARs are chunked.
    ///
    /// By default, the threshold is 128KB.
    #[serde(rename = "nar-size-threshold")]
    pub nar_size_threshold: usize,

    /// The preferred minimum size of a chunk, in bytes.
    #[serde(rename = "min-size")]
    pub min_size: usize,

    /// The preferred average size of a chunk, in bytes.
    #[serde(rename = "avg-size")]
    pub avg_size: usize,

    /// The preferred maximum size of a chunk, in bytes.
    #[serde(rename = "max-size")]
    pub max_size: usize,
}

fn default_listen_address() -> SocketAddr {
    "localhost:8080".parse().unwrap()
}
