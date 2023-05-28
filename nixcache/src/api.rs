use std::error::Error as StdError;
use std::fmt;
use anyhow::Result;
use bytes::Bytes;
use const_format::concatcp;
use serde::Deserialize;
use displaydoc::Display;
use futures::{
    future,
    stream::{self, StreamExt, TryStream, TryStreamExt},
};
use reqwest::{
    header::{HeaderValue, USER_AGENT},
    Body, Client as HttpClient, Response, StatusCode, Url,
};

use crate::config::ServerConfig;
use attic::api::v1::upload_path::{
    UploadPathNarInfo, UploadPathResult, ATTIC_NAR_INFO, ATTIC_NAR_INFO_PREAMBLE_SIZE,
};

/// The User-Agent string of Attic.
const ATTIC_USER_AGENT: &str =
    concatcp!("Nixcache {}", env!("CARGO_PKG_VERSION"));

/// The size threshold to send the upload info as part of the PUT body.
const NAR_INFO_PREAMBLE_THRESHOLD: usize = 4 * 1024; // 4 KiB

/// The Attic API client.
#[derive(Debug, Clone)]
pub struct ApiClient {
    /// Base endpoint of the server.
    endpoint: Url,
    /// An initialized HTTP client.
    client: HttpClient,
}

/// An API error.
#[derive(Debug, Display)]
pub enum ApiError {
    /// {0}
    Structured(StructuredApiError),
    /// HTTP {0}: {1}
    Unstructured(StatusCode, String),
}
#[derive(Debug, Clone, Deserialize)]
pub struct StructuredApiError {
    #[allow(dead_code)]
    code: u16,
    error: String,
    message: String,
}

impl ApiClient {
    pub fn from_server_config(config: ServerConfig) -> Result<Self> {
        let client = build_http_client();

        Ok(Self {
            endpoint: Url::parse(&config.endpoint)?,
            client,
        })
    }
    /// Uploads a path.
    pub async fn upload_path<S>(
        &self,
        nar_info: UploadPathNarInfo,
        stream: S,
        force_preamble: bool,
    ) -> Result<Option<UploadPathResult>>
    where
        S: TryStream<Ok = Bytes> + Send + Sync + 'static,
        S::Error: Into<Box<dyn StdError + Send + Sync>> + Send + Sync,
    {
        let endpoint = self.endpoint.join("api/v1/upload-path")?;
        let upload_info_json = serde_json::to_string(&nar_info)?;

        let mut req = self
            .client
            .put(endpoint)
            .header(USER_AGENT, HeaderValue::from_str(ATTIC_USER_AGENT)?);

        if force_preamble || upload_info_json.len() >= NAR_INFO_PREAMBLE_THRESHOLD {
            let preamble = Bytes::from(upload_info_json);
            let preamble_len = preamble.len();
            let preamble_stream = stream::once(future::ok(preamble));

            let chained = preamble_stream.chain(stream.into_stream());
            req = req
                .header(ATTIC_NAR_INFO_PREAMBLE_SIZE, preamble_len)
                .body(Body::wrap_stream(chained));
        } else {
            req = req
                .header(ATTIC_NAR_INFO, HeaderValue::from_str(&upload_info_json)?)
                .body(Body::wrap_stream(stream));
        }

        let res = req.send().await?;

        if res.status().is_success() {
            match res.json().await {
                Ok(r) => Ok(Some(r)),
                Err(_) => Ok(None),
            }
        } else {
            let api_error = ApiError::try_from_response(res).await?;
            Err(api_error.into())
        }
    }
}
impl StdError for ApiError {}

impl ApiError {
    async fn try_from_response(response: Response) -> Result<Self> {
        let status = response.status();
        let text = response.text().await?;
        match serde_json::from_str(&text) {
            Ok(s) => Ok(Self::Structured(s)),
            Err(_) => Ok(Self::Unstructured(status, text)),
        }
    }
}

impl fmt::Display for StructuredApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error, self.message)
    }
}

fn build_http_client() -> HttpClient {
    reqwest::Client::builder()
        .build()
        .unwrap()
}
