use std::path::PathBuf;
use serde::Deserialize;
use tokio::io::{self, AsyncRead};
use tokio::fs::{self, File};

use crate::error::{ServerError, ServerResult};
use super::{StorageBackend, RemoteFile};

#[derive(Debug)]
pub struct LocalBackend {
    config: LocalStorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalStorageConfig {
    /// The directory to store all files under.
    path: PathBuf,
}

impl LocalBackend {
    pub async fn new(config: LocalStorageConfig) -> ServerResult<Self> {
        fs::create_dir_all(&config.path)
            .await
            .map_err(ServerError::storage_error)?;

        Ok(Self { config })
    }
    fn get_path(&self, p: &str) -> PathBuf {
        self.config.path.join(p)
    }
}
#[async_trait::async_trait]
impl StorageBackend for LocalBackend {
    async fn upload_file(
        &self,
        name: String,
        mut stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile> {

        let mut file = File::create(self.get_path(&name))
            .await
            .map_err(ServerError::storage_error)?;

        io::copy(&mut stream, &mut file)
            .await
            .map_err(ServerError::storage_error)?;

        Ok(RemoteFile { path: name })
    }
}
