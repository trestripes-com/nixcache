pub mod local;

use tokio::io::AsyncRead;

use crate::error::ServerResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteFile {
    Chunk(String),
    Nar(String),
}

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync + std::fmt::Debug {
    /// Uploads a chunk.
    async fn upload_chunk(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile>;

    /// Uploads a NAR.
    async fn upload_nar(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile>;
}
