use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use serde::{Serialize, Deserialize};
use aws_sdk_s3::{
    operation::get_object::builders::GetObjectFluentBuilder,
    config::Builder as S3ConfigBuilder,
    types::{CompletedMultipartUpload, CompletedPart},
    config::{Credentials, Region},
    Client,
};
use bytes::BytesMut;
use futures::future::join_all;
use futures::stream::StreamExt;
use tokio::io::AsyncRead;

use crate::finally::Finally;
use crate::chunking::read_chunk_async;
use crate::error::{ServerResult, ServerError};
use super::{StorageBackend, RemoteFile, Download};

/// The chunk size for each part in a multipart upload.
const CHUNK_SIZE: usize = 8 * 1024 * 1024;

/// The S3 remote file storage backend.
#[derive(Debug)]
pub struct S3Backend {
    client: Client,
    config: S3StorageConfig,
}

/// S3 remote file storage configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct S3StorageConfig {
    /// The AWS region.
    region: String,
    /// The name of the bucket.
    bucket: String,
    /// Custom S3 endpoint.
    ///
    /// Set this if you are using an S3-compatible object storage (e.g., Minio).
    endpoint: Option<String>,
    /// S3 credentials.
    credentials: Option<S3CredentialsConfig>,

    /// Dir name for chunks.
    #[serde(default = "default_chunks_dir_name")]
    chunks: String,
    /// Dir name for NARs.
    #[serde(default = "default_nars_dir_name")]
    nars: String,
}

/// S3 credential configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct S3CredentialsConfig {
    /// Access key ID.
    access_key_id: String,
    /// Secret access key.
    secret_access_key: String,
}

/// Reference to a file in an S3-compatible storage bucket.
///
/// We store the region and bucket to facilitate migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3RemoteFile {
    /// Name of the S3 region.
    pub region: String,
    /// Name of the bucket.
    pub bucket: String,
    /// Key of the file.
    pub key: String,
}

impl S3Backend {
    pub async fn new(config: S3StorageConfig) -> ServerResult<Self> {
        let s3_config = Self::config_builder(&config)
            .await?
            .region(Region::new(config.region.to_owned()))
            .build();

        Ok(Self {
            client: Client::from_conf(s3_config),
            config,
        })
    }
    async fn config_builder(config: &S3StorageConfig) -> ServerResult<S3ConfigBuilder> {
        let mut builder = S3ConfigBuilder::new();

        if let Some(credentials) = &config.credentials {
            builder = builder.credentials_provider(Credentials::new(
                &credentials.access_key_id,
                &credentials.secret_access_key,
                None,
                None,
                "s3",
            ));
        }

        if let Some(endpoint) = &config.endpoint {
            builder = builder.endpoint_url(endpoint).force_path_style(true);
        }

        Ok(builder)
    }

    async fn upload_file(
        &self,
        name: String,
        mut stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile> {
        let buf = BytesMut::with_capacity(CHUNK_SIZE);
        let first_chunk = read_chunk_async(&mut stream, buf)
            .await
            .map_err(ServerError::storage_error)?;

        if first_chunk.len() < CHUNK_SIZE {
            // do a normal PutObject
            let put_object = self
                .client
                .put_object()
                .bucket(&self.config.bucket)
                .key(&name)
                .body(first_chunk.into())
                .send()
                .await
                .map_err(ServerError::storage_error)?;

            tracing::debug!("put_object -> {:#?}", put_object);

            return Ok(RemoteFile::S3(S3RemoteFile {
                region: self.config.region.clone(),
                bucket: self.config.bucket.clone(),
                key: name,
            }));
        }

        let multipart = self
            .client
            .create_multipart_upload()
            .bucket(&self.config.bucket)
            .key(&name)
            .send()
            .await
            .map_err(ServerError::storage_error)?;

        let upload_id = multipart.upload_id().unwrap();

        let cleanup = Finally::new({
            let bucket = self.config.bucket.clone();
            let client = self.client.clone();
            let upload_id = upload_id.to_owned();
            let name = name.clone();

            async move {
                tracing::warn!("Upload was interrupted - Aborting multipart upload");

                let r = client
                    .abort_multipart_upload()
                    .bucket(bucket)
                    .key(name)
                    .upload_id(upload_id)
                    .send()
                    .await;

                if let Err(e) = r {
                    tracing::warn!("Failed to abort multipart upload: {}", e);
                }
            }
        });
        let mut part_number = 1;
        let mut parts = Vec::new();
        let mut first_chunk = Some(first_chunk);

        loop {
            let chunk = if part_number == 1 {
                first_chunk.take().unwrap()
            } else {
                let buf = BytesMut::with_capacity(CHUNK_SIZE);
                read_chunk_async(&mut stream, buf)
                    .await
                    .map_err(ServerError::storage_error)?
            };

            if chunk.is_empty() {
                break;
            }

            let client = self.client.clone();
            let fut = tokio::task::spawn({
                client
                    .upload_part()
                    .bucket(&self.config.bucket)
                    .key(&name)
                    .upload_id(upload_id)
                    .part_number(part_number)
                    .body(chunk.clone().into())
                    .send()
            });

            parts.push(fut);
            part_number += 1;
        }

        let completed_parts = join_all(parts)
            .await
            .into_iter()
            .map(|join_result| join_result.unwrap())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(ServerError::storage_error)?
            .into_iter()
            .enumerate()
            .map(|(idx, part)| {
                let part_number = idx + 1;
                CompletedPart::builder()
                    .set_e_tag(part.e_tag().map(str::to_string))
                    .set_part_number(Some(part_number as i32))
                    .set_checksum_crc32(part.checksum_crc32().map(str::to_string))
                    .set_checksum_crc32_c(part.checksum_crc32_c().map(str::to_string))
                    .set_checksum_sha1(part.checksum_sha1().map(str::to_string))
                    .set_checksum_sha256(part.checksum_sha256().map(str::to_string))
                    .build()
            })
            .collect::<Vec<_>>();

        let completed_multipart_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        let completion = self
            .client
            .complete_multipart_upload()
            .bucket(&self.config.bucket)
            .key(&name)
            .upload_id(upload_id)
            .multipart_upload(completed_multipart_upload)
            .send()
            .await
            .map_err(ServerError::storage_error)?;

        tracing::debug!("complete_multipart_upload -> {:#?}", completion);

        cleanup.cancel();

        Ok(RemoteFile::S3(S3RemoteFile {
            region: self.config.region.clone(),
            bucket: self.config.bucket.clone(),
            key: name,
        }))
    }

    async fn download_file(&self, name: String) -> ServerResult<Download> {
        let req = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(&name);

        self.get_download(req).await
    }
    async fn get_download(&self, req: GetObjectFluentBuilder) -> ServerResult<Download> {
        let output = req.send().await.map_err(ServerError::storage_error)?;

        let stream = StreamExt::map(output.body, |item| {
            item.map_err(|e| IoError::new(IoErrorKind::Other, e))
        });

        Ok(Download::Stream(Box::pin(stream)))
    }

    fn get_chunk_path(&self, p: &str) -> String {
        format!("{}/{}", self.config.chunks, p)
    }
    fn get_nar_path(&self, p: &str) -> String {
        format!("{}/{}", self.config.nars, p)
    }
}
#[async_trait::async_trait]
impl StorageBackend for S3Backend {
    async fn upload_chunk(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile> {
        self.upload_file(self.get_chunk_path(&name), stream).await
    }
    async fn upload_nar(
        &self,
        name: String,
        stream: &mut (dyn AsyncRead + Unpin + Send),
    ) -> ServerResult<RemoteFile> {
        self.upload_file(self.get_nar_path(&name), stream).await
    }

    async fn download_chunk(
        &self,
        name: String,
    ) -> ServerResult<Download> {
        self.download_file(self.get_chunk_path(&name)).await
    }
    async fn download_nar(
        &self,
        name: String,
    ) -> ServerResult<Download> {
        self.download_file(self.get_nar_path(&name)).await
    }
}

fn default_chunks_dir_name() -> String {
    "chunks".to_string()
}
fn default_nars_dir_name() -> String {
    "nars".to_string()
}
