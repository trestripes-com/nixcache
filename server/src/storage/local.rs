use anyhow::Result;
use std::path::PathBuf;
use serde::Deserialize;
use tokio::io::{self, AsyncRead};
use tokio::fs::{self, File};

use crate::error::{ServerError, ServerResult};
use super::{StorageBackend, RemoteFile, Download};

#[derive(Debug)]
pub struct LocalBackend {
    config: LocalStorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalStorageConfig {
    /// The directory to store all files under.
    path: PathBuf,
    /// Dir name for chunks.
    #[serde(default = "default_chunks_dir_name")]
    chunks: String,
    /// Dir name for NARs.
    #[serde(default = "default_nars_dir_name")]
    nars: String,
}
impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            path: "/tmp/_nixcache".into(),
            chunks: default_chunks_dir_name(),
            nars: default_nars_dir_name(),
        }
    }
}

impl LocalBackend {
    pub async fn new(config: LocalStorageConfig) -> Result<Self> {
        fs::create_dir_all(&config.path.join(&config.chunks))
            .await?;
        fs::create_dir_all(&config.path.join(&config.nars))
            .await?;

        Ok(Self { config })
    }
    fn get_chunk_path(&self, p: &str) -> PathBuf {
        self.config.path.join(&self.config.chunks).join(p)
    }
    fn get_nar_path(&self, p: &str) -> PathBuf {
        self.config.path.join(&self.config.nars).join(p)
    }
    async fn upload(
        &self,
        path: PathBuf,
        mut stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<()> {
        let mut file = File::create(path)
            .await
            .map_err(ServerError::storage_error)?;

        io::copy(&mut stream, &mut file)
            .await
            .map_err(ServerError::storage_error)?;

        Ok(())
    }
}
#[async_trait::async_trait]
impl StorageBackend for LocalBackend {
    async fn upload_chunk(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile> {
        self.upload(self.get_chunk_path(&name), stream).await?;
        Ok(RemoteFile::Chunk(name))
    }
    async fn upload_nar(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile> {
        self.upload(self.get_nar_path(&name), stream).await?;
        Ok(RemoteFile::Nar(name))
    }
    async fn download_chunk(
        &self,
        name: String,
    ) -> ServerResult<Download> {
        let file = File::open(self.get_chunk_path(&name))
            .await
            .map_err(ServerError::storage_error)?;

        Ok(Download::AsyncRead(Box::new(file)))
    }
    async fn download_nar(
        &self,
        name: String,
    ) -> ServerResult<Download> {
        let file = File::open(self.get_nar_path(&name))
            .await
            .map_err(ServerError::storage_error)?;

        Ok(Download::AsyncRead(Box::new(file)))
    }
}

fn default_chunks_dir_name() -> String {
    "chunks".to_string()
}
fn default_nars_dir_name() -> String {
    "nars".to_string()
}
