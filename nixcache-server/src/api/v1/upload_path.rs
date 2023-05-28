use std::io;
use std::io::Cursor;
use std::marker::Unpin;
use std::sync::Arc;
use anyhow::anyhow;
use async_compression::tokio::bufread::{BrotliEncoder, XzEncoder, ZstdEncoder};
use async_compression::Level as CompressionLevel;
use axum::{extract::{BodyStream, Extension, Json}, http::HeaderMap};
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use digest::Output as DigestOutput;
use futures::future::join_all;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, BufReader};
use tokio::sync::{OnceCell, Semaphore};
use tokio::task::spawn;
use tokio_util::io::StreamReader;
use tracing::instrument;

use crate::config::CompressionType;
use crate::error::{ErrorKind, ServerError, ServerResult};
use crate::narinfo::Compression;
use crate::State;

use attic::api::v1::upload_path::{
    UploadPathNarInfo, UploadPathResult, UploadPathResultKind, ATTIC_NAR_INFO,
    ATTIC_NAR_INFO_PREAMBLE_SIZE,
};
use attic::hash::Hash;
use attic::stream::{read_chunk_async, StreamHasher};

use crate::chunking::chunk_stream;

/// Number of chunks to upload to the storage backend at once.
const CONCURRENT_CHUNK_UPLOADS: usize = 10;

/// The maximum size of the upload info JSON.
const MAX_NAR_INFO_SIZE: usize = 1 * 1024 * 1024; // 1 MiB

type CompressorFn<C> = Box<dyn FnOnce(C) -> Box<dyn AsyncRead + Unpin + Send> + Send>;

struct UploadedChunk {
    file_hash: Hash,
    file_size: usize,
    compression_type: CompressionType,
    compression_level: CompressionLevel,
}

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
) -> ServerResult<Json<UploadPathResult>> {
    let mut stream = StreamReader::new(
        stream.map(|r| r.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))),
    );

    let upload_info: UploadPathNarInfo = {
        if let Some(preamble_size_bytes) = headers.get(ATTIC_NAR_INFO_PREAMBLE_SIZE) {
            // Read from the beginning of the PUT body
            let preamble_size: usize = preamble_size_bytes
                .to_str()
                .map_err(|_| {
                    ErrorKind::RequestError(anyhow!(
                        "{} has invalid encoding",
                        ATTIC_NAR_INFO_PREAMBLE_SIZE
                    ))
                })?
                .parse()
                .map_err(|_| {
                    ErrorKind::RequestError(anyhow!(
                        "{} must be a valid unsigned integer",
                        ATTIC_NAR_INFO_PREAMBLE_SIZE
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
        } else if let Some(nar_info_bytes) = headers.get(ATTIC_NAR_INFO) {
            // Read from X-Attic-Nar-Info header
            serde_json::from_slice(nar_info_bytes.as_bytes()).map_err(ServerError::request_error)?
        } else {
            return Err(ErrorKind::RequestError(anyhow!("{} must be set", ATTIC_NAR_INFO)).into());
        }
    };

    upload_path_new(upload_info, stream, &state).await
}

/// Uploads a path when there is no matching NAR in the global cache.
///
/// It's okay if some other client races to upload the same NAR before
/// us. The `nar` table can hold duplicate NARs which can be deduplicated
/// in a background process.
async fn upload_path_new(
    upload_info: UploadPathNarInfo,
    stream: impl AsyncRead + Send + Unpin + 'static,
    state: &State,
) -> ServerResult<Json<UploadPathResult>> {
    let nar_size_threshold = state.config.chunking.nar_size_threshold;

    if nar_size_threshold == 0 || upload_info.nar_size < nar_size_threshold {
        upload_path_new_unchunked(upload_info, stream, state).await
    } else {
        upload_path_new_chunked(upload_info, stream, state).await
    }
}

/// Uploads a path when there is no matching NAR in the global cache (unchunked).
///
/// We upload the entire NAR as a single chunk.
async fn upload_path_new_unchunked(
    upload_info: UploadPathNarInfo,
    stream: impl AsyncRead + Send + Unpin + 'static,
    state: &State,
) -> ServerResult<Json<UploadPathResult>> {
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
        .upload_file(file_hash.to_typed_base32(), &mut Cursor::new(read))
        .await?;

    let chunk = UploadedChunk {
        file_hash,
        file_size: *file_size,
        compression_type,
        compression_level,
    };

    // TODO Create a NAR entry
    //     let model = nar::ActiveModel {
    //         nar_hash: Set(upload_info.nar_hash.to_typed_base16()),
    //         nar_size: Set(chunk.guard.chunk_size),

    //         num_chunks: Set(1),

    //         created_at: Set(Utc::now()),
    //         ..Default::default()
    //     };

    Ok(Json(UploadPathResult {
        kind: UploadPathResultKind::Uploaded,
        file_size: Some(chunk.file_size),
        frac_deduplicated: None,
    }))
}

/// Uploads a path when there is no matching NAR in the global cache (chunked).
async fn upload_path_new_chunked(
    upload_info: UploadPathNarInfo,
    stream: impl AsyncRead + Send + Unpin + 'static,
    state: &State,
) -> ServerResult<Json<UploadPathResult>> {
    let chunking_config = &state.config.chunking;
    let compression_config = &state.config.compression;
    let compression_type = compression_config.r#type;
    let compression_level = compression_config.level();
    let compression: Compression = compression_type.into();

    let nar_size_db = i64::try_from(upload_info.nar_size).map_err(ServerError::request_error)?;

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

    let mut chunk_idx = 0;
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

            spawn(async move {
                let read = read_chunk_async(&mut stream.stream(), buf)
                    .await
                    .map_err(ServerError::request_error)?;

                let (file_hash, file_size) = stream.file_hash_and_size().unwrap();
                let file_hash = Hash::Sha256(file_hash.as_slice().try_into().unwrap());

                // Upload chunk
                let backend = state.storage();
                backend
                    .upload_file(file_hash.to_typed_base32(), &mut Cursor::new(read))
                    .await?;

                let chunk = UploadedChunk {
                    file_hash,
                    file_size: *file_size,
                    compression_type,
                    compression_level,
                };

                drop(permit);
                Ok(chunk)
            })
        });

        chunk_idx += 1;
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

    // let model = nar::ActiveModel {
    //     compression: Set(compression.to_string()),

    //     nar_hash: Set(upload_info.nar_hash.to_typed_base16()),
    //     nar_size: Set(nar_size_db),

    //     num_chunks: Set(0),

    //     created_at: Set(Utc::now()),
    //     ..Default::default()
    // };
    // // Set num_chunks and mark the NAR as Valid
    // Nar::update(nar::ActiveModel {
        // id: Set(nar_id),
        // state: Set(NarState::Valid),
        // num_chunks: Set(chunks.len() as i32),
        // ..Default::default()
    // })

    // // Create a mapping granting the local cache access to the NAR
    // Object::insert({
        // let mut new_object = upload_info.to_active_model();
        // new_object.cache_id = Set(cache.id);
        // new_object.nar_id = Set(nar_id);
        // new_object.created_at = Set(Utc::now());
        // new_object.created_by = Set(username);
        // new_object
    // })

    Ok(Json(UploadPathResult {
        kind: UploadPathResultKind::Uploaded,
        file_size: Some(file_size),
        frac_deduplicated: None,
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
