//! Nix Binary Cache server.
//!
//! This module implements the Nix Binary Cache API.
//!
//! The implementation is based on the specifications at <https://github.com/fzakaria/nix-http-binary-cache-api-spec>.

use anyhow::anyhow;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::VecDeque;
use axum::{
    body::StreamBody,
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, head},
    Router,
};
use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use tracing::instrument;

use libnixstore::StorePathHash;
use common::mime;
use crate::error::{ErrorKind, ServerResult, ServerError};
use crate::{nix_manifest, State, narinfo::NarInfo};
use crate::storage::{StorageBackend, Download};
use crate::api::{UploadedNar, UploadedChunk};
use crate::chunking::merge_chunks;

/// Nix cache information.
///
/// An example of a correct response is as follows:
///
/// ```text
/// StoreDir: /nix/store
/// WantMassQuery: 1
/// Priority: 40
/// ```
#[derive(Debug, Clone, Serialize)]
struct NixCacheInfo {
    /// Whether this binary cache supports bulk queries.
    #[serde(rename = "WantMassQuery")]
    want_mass_query: bool,
    /// The Nix store path this binary cache uses.
    #[serde(rename = "StoreDir")]
    store_dir: PathBuf,
    /// The priority of the binary cache.
    ///
    /// A lower number denotes a higher priority.
    /// <https://cache.nixos.org> has a priority of 40.
    #[serde(rename = "Priority")]
    priority: i32,
}
impl IntoResponse for NixCacheInfo {
    fn into_response(self) -> Response {
        match nix_manifest::to_string(&self) {
            Ok(body) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime::NIX_CACHE_INFO)
                .body(body)
                .unwrap()
                .into_response(),
            Err(e) => e.into_response(),
        }
    }
}

/// Gets information on a cache.
#[instrument(skip_all)]
async fn get_nix_cache_info() -> ServerResult<NixCacheInfo> {
    let info = NixCacheInfo {
        want_mass_query: true,
        store_dir: super::v1::CACHE_STOREDIR.into(),
        priority: super::v1::CACHE_PRIORITY,
    };
    Ok(info)
}

/// Gets various information on a store path hash.
///
/// `/:path`, which may be one of
/// - GET  `/{storePathHash}.narinfo`
/// - HEAD `/{storePathHash}.narinfo`
/// - GET  `/{storePathHash}.ls`      (NOT IMPLEMENTED)
#[instrument(skip_all, fields(path))]
#[axum_macros::debug_handler]
async fn get_store_path_info(
    Extension(state): Extension<Arc<State>>,
    Path(path): Path<String>,
) -> ServerResult<NarInfo> {
    let components: Vec<&str> = path.splitn(2, '.').collect();
    if components.len() != 2 {
        return Err(ErrorKind::NotFound.into());
    }
    if components[1] != "narinfo" {
        return Err(ErrorKind::NotFound.into());
    }
    let store_path_hash = StorePathHash::new(components[0].to_string())
        .map_err(|e| ErrorKind::RequestError(anyhow!(
            "Could not parse store path hash : {}", e
        )))?;

    tracing::debug!("Received request for {}.narinfo", store_path_hash.as_str());

    // Get NAR
    let backend = state.storage();
    let nar = backend
        .download_nar(store_path_hash.to_string())
        .await?;

    let mut narinfo = match nar {
        Download::AsyncRead(mut stream) => {
            let mut nar = Vec::new();
            stream.read_to_end(&mut nar).await
                .map_err(ServerError::storage_error)?;
            let nar: UploadedNar = serde_json::from_slice(&nar)
                .map_err(ServerError::storage_error)?;
            nar.into_narinfo(&store_path_hash)
        },
        Download::Stream(stream) => {
            use futures::AsyncReadExt;

            let mut nar = Vec::new();
            stream.into_async_read().read_to_end(&mut nar).await
                .map_err(ServerError::storage_error)?;
            let nar: UploadedNar = serde_json::from_slice(&nar)
                .map_err(ServerError::storage_error)?;
            nar.into_narinfo(&store_path_hash)
        },
    };

    if narinfo.signature().is_none() {
        narinfo.sign(&state.config.keypair);
    }

    Ok(narinfo)
}

/// Gets a NAR.
///
/// - GET `:cache/nar/{storePathHash}.nar`
///
/// Here we use the store path hash not the NAR hash or file hash
/// for better logging. In reality, the files are deduplicated by
/// content-addressing.
#[instrument(skip_all, fields(cache_name, path))]
async fn get_nar(
    Extension(state): Extension<Arc<State>>,
    Path(path): Path<String>,
) -> ServerResult<Response> {
    let components: Vec<&str> = path.splitn(2, '.').collect();
    if components.len() != 2 {
        return Err(ErrorKind::NotFound.into());
    }
    if components[1] != "nar" {
        return Err(ErrorKind::NotFound.into());
    }
    let store_path_hash = StorePathHash::new(components[0].to_string())
        .map_err(|e| ErrorKind::RequestError(anyhow!(
            "Could not parse store path hash : {}", e
        )))?;

    tracing::debug!("Received request for {}.nar", store_path_hash.as_str());

    // Get NAR
    let backend = state.storage();

    let nar = backend
        .download_nar(store_path_hash.to_string())
        .await?;

    let nar: UploadedNar = match nar {
        Download::AsyncRead(mut stream) => {
            let mut nar = Vec::new();
            stream.read_to_end(&mut nar).await
                .map_err(ServerError::storage_error)?;
            serde_json::from_slice(&nar)
                .map_err(ServerError::storage_error)?
        },
        Download::Stream(stream) => {
            use futures::AsyncReadExt;

            let mut nar = Vec::new();
            stream.into_async_read().read_to_end(&mut nar).await
                .map_err(ServerError::storage_error)?;
            serde_json::from_slice(&nar)
                .map_err(ServerError::storage_error)?
        },
    };

    // Stream merged chunks
    if nar.chunks.len() == 1 {
        // single chunk
        let chunk = &nar.chunks[0];
        match backend.download_chunk(chunk.file_hash.to_typed_base32()).await? {
            Download::AsyncRead(stream) => {
                let stream = ReaderStream::new(stream);
                let body = StreamBody::new(stream);
                Ok(body.into_response())
            },
            Download::Stream(stream) => {
                let body = StreamBody::new(stream);
                Ok(body.into_response())
            },
        }
    } else {
        // reassemble NAR

        fn io_error<E: std::error::Error + Send + Sync + 'static>(e: E) -> IoError {
            IoError::new(IoErrorKind::Other, e)
        }

        let streamer = |chunk: UploadedChunk, storage: Arc<Box<dyn StorageBackend + 'static>>| async move {
            match storage
                .download_chunk(chunk.file_hash.to_typed_base32())
                .await
                .map_err(io_error)?
            {
                Download::AsyncRead(stream) => {
                    let stream: BoxStream<_> = Box::pin(ReaderStream::new(stream));
                    Ok(stream)
                },
                Download::Stream(stream) => Ok(stream),
            }
        };

        let chunks: VecDeque<_> = nar.chunks.into();

        // TODO: Make num_prefetch configurable
        // The ideal size depends on the average chunk size
        let merged = merge_chunks(chunks, streamer, backend, 2);
        let body = StreamBody::new(merged);
        Ok(body.into_response())
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/nix-cache-info", get(get_nix_cache_info))
        .route("/:path", head(get_store_path_info))
        .route("/:path", get(get_store_path_info))
        .route("/nar/:path", get(get_nar))
}
