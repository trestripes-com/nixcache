pub mod local;

use tokio::io::AsyncRead;

use crate::error::ServerResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    path: String,
}

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync + std::fmt::Debug {
    /// Uploads a file.
    async fn upload_file(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile>;
}