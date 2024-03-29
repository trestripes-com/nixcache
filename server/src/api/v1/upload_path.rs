use std::io;
use std::io::Cursor;
use std::marker::Unpin;
use std::sync::Arc;
use std::path::PathBuf;
use anyhow::anyhow;
use async_compression::tokio::bufread::{BrotliEncoder, XzEncoder, ZstdEncoder};
use async_compression::Level as CompressionLevel;
use axum::{extract::{BodyStream, Extension, Json}, http::HeaderMap};
use bytes::BytesMut;
use digest::Output as DigestOutput;
use futures::future::join_all;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, BufReader};
use tokio::sync::{OnceCell, Semaphore};
use tokio::task::spawn;
use tokio_util::io::StreamReader;
use tracing::instrument;

use libnixstore::Hash;
use common::v1::header;
use common::v1::upload_path::{Request, Response, ResponseKind};
use crate::config::CompressionType;
use crate::error::{ErrorKind, ServerError, ServerResult};
use crate::State;
use crate::chunking::{chunk_stream, read_chunk_async};
use crate::stream::StreamHasher;
use crate::api::{UploadedChunk, UploadedNar};

/// Number of chunks to upload to the storage backend at once.
const CONCURRENT_CHUNK_UPLOADS: usize = 10;

/// The maximum size of the upload info JSON.
const MAX_NAR_INFO_SIZE: usize = 1 * 1024 * 1024; // 1 MiB

type CompressorFn<C> = Box<dyn FnOnce(C) -> Box<dyn AsyncRead + Unpin + Send> + Send>;

/// Applies compression to a stream, computing hashes along the way.
///
/// ```text
///                ┌──────────┐  ┌───────────┐
/// Chunk Stream──►│Compressor├─►│File Hasher├─►File Stream
///                └──────────┘  └─────┬─────┘
///                                    ├───────►File Hash
///                                    └───────►File Size
/// ```
struct CompressionStream {
    stream: Box<dyn AsyncRead + Unpin + Send>,
    file_compute: Arc<OnceCell<(DigestOutput<Sha256>, usize)>>,
}

/// Uploads a new object to the cache.
#[instrument(skip_all)]
#[axum_macros::debug_handler]
pub async fn upload_path(
    Extension(state): Extension<Arc<State>>,
    headers: HeaderMap,
    stream: BodyStream,
) -> ServerResult<Json<Response>> {
    let mut stream = StreamReader::new(
        stream.map(|r| r.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))),
    );

    let upload_info: Request = {
        if let Some(preamble_size_bytes) = headers.get(header::NAR_INFO_PREAMBLE_SIZE) {
            // Read from the beginning of the PUT body
            let preamble_size: usize = preamble_size_bytes
                .to_str()
                .map_err(|_| {
                    ErrorKind::RequestError(anyhow!(
                        "{} has invalid encoding",
                        header::NAR_INFO_PREAMBLE_SIZE
                    ))
                })?
                .parse()
                .map_err(|_| {
                    ErrorKind::RequestError(anyhow!(
                        "{} must be a valid unsigned integer",
                        header::NAR_INFO_PREAMBLE_SIZE
                    ))
                })?;

            if preamble_size > MAX_NAR_INFO_SIZE {
                return Err(ErrorKind::RequestError(anyhow!("Upload info is too large")).into());
            }

            let buf = BytesMut::with_capacity(preamble_size);
            let preamble = read_chunk_async(&mut stream, buf)
                .await
                .map_err(|e| ErrorKind::RequestError(e.into()))?;

            if preamble.len() != preamble_size {
                return Err(ErrorKind::RequestError(anyhow!(
                    "Upload info doesn't match specified size"
                ))
                .into());
            }

            serde_json::from_slice(&preamble).map_err(ServerError::request_error)?
        } else if let Some(nar_info_bytes) = headers.get(header::NAR_INFO) {
            // Read from X-Attic-Nar-Info header
            serde_json::from_slice(nar_info_bytes.as_bytes()).map_err(ServerError::request_error)?
        } else {
            return Err(ErrorKind::RequestError(anyhow!("{} must be set", header::NAR_INFO)).into());
        }
    };

    upload_path_new(upload_info, stream, &state).await
}

/// Uploads a path when there is no matching NAR in the global cache.
async fn upload_path_new(
    upload_info: Request,
    stream: impl AsyncRead + Send + Unpin + 'static,
    state: &State,
) -> ServerResult<Json<Response>> {
    let nar_size_threshold = state.config.chunking.nar_size_threshold;

    if nar_size_threshold == 0 || upload_info.nar_size < nar_size_threshold {
        upload_path_new_unchunked(upload_info, stream, state).await
    } else {
        upload_path_new_chunked(upload_info, stream, state).await
    }
}

/// Upload the entire NAR as a single chunk.
async fn upload_path_new_unchunked(
    upload_info: Request,
    stream: impl AsyncRead + Send + Unpin + 'static,
    state: &State,
) -> ServerResult<Json<Response>> {
    let compression_config = &state.config.compression;
    let compression_type = compression_config.r#type;
    let compression_level = compression_config.level();

    let stream = stream.take(upload_info.nar_size as u64);
    let (stream, nar_compute) = StreamHasher::new(stream, Sha256::new());

    // Compress
    let compressor = get_compressor_fn(compression_type, compression_level);
    let mut stream = CompressionStream::new(stream, compressor);

    let buf = BytesMut::with_capacity(state.config.chunking.max_size);
    let read = read_chunk_async(&mut stream.stream(), buf)
        .await
        .map_err(ServerError::request_error)?;

    // Confirm that the chunk hash is correct
    let (nar_hash, nar_size) = nar_compute.get().unwrap();
    let (file_hash, file_size) = stream.file_hash_and_size().unwrap();

    let nar_hash = Hash::Sha256(nar_hash.as_slice().try_into().unwrap());
    let file_hash = Hash::Sha256(file_hash.as_slice().try_into().unwrap());

    if upload_info.nar_hash != nar_hash || upload_info.nar_size != *nar_size {
        return Err(ErrorKind::RequestError(anyhow!("Bad chunk hash or size")).into());
    }

    // Upload chunk
    let backend = state.storage();

    backend
        .upload_chunk(file_hash.to_typed_base32(), &mut Cursor::new(read))
        .await?;

    let chunks = vec![UploadedChunk {
        file_hash,
        file_size: *file_size,
        compression: compression_config.clone(),
    }];

    // Upload NAR
    let nar = UploadedNar {
        nar_hash,
        nar_size: *nar_size,
        chunks,
        ca: upload_info.ca,
        references: upload_info.references,
        store_path: PathBuf::from(upload_info.store_path),
        system: upload_info.system,
    };
    let data = serde_json::to_vec(&nar)
        .map_err(ServerError::storage_error)?;

    backend
        .upload_nar(upload_info.store_path_hash.to_string(), &mut Cursor::new(data))
        .await?;

    Ok(Json(Response {
        kind: ResponseKind::Uploaded,
        file_size: Some(*file_size),
    }))
}

/// Uploads chunked NAR.
async fn upload_path_new_chunked(
    upload_info: Request,
    stream: impl AsyncRead + Send + Unpin + 'static,
    state: &State,
) -> ServerResult<Json<Response>> {
    let chunking_config = &state.config.chunking;
    let compression_config = &state.config.compression;
    let compression_type = compression_config.r#type;
    let compression_level = compression_config.level();

    let stream = stream.take(upload_info.nar_size as u64);
    let (stream, nar_compute) = StreamHasher::new(stream, Sha256::new());
    let mut chunks = chunk_stream(
        stream,
        chunking_config.min_size,
        chunking_config.avg_size,
        chunking_config.max_size,
    );

    let upload_chunk_limit = Arc::new(Semaphore::new(CONCURRENT_CHUNK_UPLOADS));
    let mut futures = Vec::new();

    while let Some(bytes) = chunks.next().await {
        let data = bytes.map_err(ServerError::request_error)?;

        // Wait for a permit before spawning
        //
        // We want to block the receive process as well, otherwise it stays ahead and
        // consumes too much memory
        let permit = upload_chunk_limit.clone().acquire_owned().await.unwrap();
        futures.push({
            let state = state.clone();

            let compressor = get_compressor_fn(compression_type, compression_level);
            let mut stream = CompressionStream::new(Cursor::new(data), compressor);
            let buf = BytesMut::with_capacity(state.config.chunking.max_size);

            let compression = compression_config.clone();
            spawn(async move {
                let read = read_chunk_async(&mut stream.stream(), buf)
                    .await
                    .map_err(ServerError::request_error)?;

                let (file_hash, file_size) = stream.file_hash_and_size().unwrap();
                let file_hash = Hash::Sha256(file_hash.as_slice().try_into().unwrap());

                // Upload chunk
                let backend = state.storage();
                backend
                    .upload_chunk(file_hash.to_typed_base32(), &mut Cursor::new(read))
                    .await?;

                let chunk = UploadedChunk {
                    file_hash,
                    file_size: *file_size,
                    compression,
                };

                drop(permit);
                Ok(chunk)
            })
        });
    }

    // Confirm that the NAR Hash and Size are correct
    let (nar_hash, nar_size) = nar_compute.get().unwrap();
    let nar_hash = Hash::Sha256(nar_hash.as_slice().try_into().unwrap());

    if nar_hash != upload_info.nar_hash || *nar_size != upload_info.nar_size {
        return Err(ErrorKind::RequestError(anyhow!("Bad NAR Hash or Size")).into());
    }

    // Wait for all uploads to complete
    let chunks: Vec<UploadedChunk> = join_all(futures)
        .await
        .into_iter()
        .map(|join_result| join_result.unwrap())
        .collect::<ServerResult<Vec<_>>>()?;

    let file_size = chunks
        .iter()
        .fold(0, |file_size, chunk| file_size + chunk.file_size);

    // Upload NAR
    let nar = UploadedNar {
        nar_hash,
        nar_size: *nar_size,
        chunks,
        ca: upload_info.ca,
        references: upload_info.references,
        store_path: PathBuf::from(upload_info.store_path),
        system: upload_info.system,
    };
    let data = serde_json::to_vec(&nar)
        .map_err(ServerError::storage_error)?;

    let backend = state.storage();
    backend
        .upload_nar(upload_info.store_path_hash.to_string(), &mut Cursor::new(data))
        .await?;

    Ok(Json(Response {
        kind: ResponseKind::Uploaded,
        file_size: Some(file_size),
    }))
}

/// Returns a compressor function that takes some stream as input.
fn get_compressor_fn<C: AsyncBufRead + Unpin + Send + 'static>(
    ctype: CompressionType,
    level: CompressionLevel,
) -> CompressorFn<C> {
    match ctype {
        CompressionType::None => Box::new(|c| Box::new(c)),
        CompressionType::Brotli => {
            Box::new(move |s| Box::new(BrotliEncoder::with_quality(s, level)))
        }
        CompressionType::Zstd => Box::new(move |s| Box::new(ZstdEncoder::with_quality(s, level))),
        CompressionType::Xz => Box::new(move |s| Box::new(XzEncoder::with_quality(s, level))),
    }
}

impl CompressionStream {
    /// Creates a new compression stream.
    fn new<R>(stream: R, compressor: CompressorFn<BufReader<R>>) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let stream = compressor(BufReader::new(stream));

        // compute file hash and size
        let (stream, file_compute) = StreamHasher::new(stream, Sha256::new());

        Self {
            stream: Box::new(stream),
            file_compute,
        }
    }

    /// Returns the stream of the compressed object.
    fn stream(&mut self) -> &mut (impl AsyncRead + Unpin) {
        &mut self.stream
    }

    /// Returns the file hash and size.
    ///
    /// The hash is only finalized when the stream is fully read.
    /// Otherwise, returns `None`.
    fn file_hash_and_size(&self) -> Option<&(DigestOutput<Sha256>, usize)> {
        self.file_compute.get()
    }
}
