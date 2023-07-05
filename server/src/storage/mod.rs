pub mod local;
pub mod s3;

use bytes::Bytes;
use futures::stream::BoxStream;
use tokio::io::AsyncRead;

use crate::error::ServerResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteFile {
    Local(local::LocalRemoteFile),
    S3(s3::S3RemoteFile),
}

/// Way to download a file.
pub enum Download {
    Stream(BoxStream<'static, std::io::Result<Bytes>>),
    AsyncRead(Box<dyn AsyncRead + Unpin + Send>),
}

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync + std::fmt::Debug {
    /// Uploads a chunk.
    async fn upload_chunk(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile>;
    /// Downloads a chunk.
    async fn download_chunk(
        &self,
        name: String,
    ) -> ServerResult<Download>;

    /// Uploads a NAR.
    async fn upload_nar(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile>;
    /// Downloads a NAR.
    async fn download_nar(
        &self,
        name: String,
    ) -> ServerResult<Download>;
}
