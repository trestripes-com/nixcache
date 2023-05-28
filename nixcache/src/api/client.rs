use std::error::Error as StdError;
use anyhow::Result;
use bytes::Bytes;
use const_format::concatcp;
use futures::{
    future,
    stream::{self, StreamExt, TryStream, TryStreamExt},
};
use reqwest::{header::HeaderValue, Body, Client as HttpClient, Url};

use crate::config::ServerConfig;
use super::error::Error;
use nixcache_common::v1::header;
use nixcache_common::v1::upload_path;

/// The User-Agent string.
const USER_AGENT: &str = concatcp!("Nixcache {}", env!("CARGO_PKG_VERSION"));

/// The size threshold to send the upload info as part of the PUT body.
const NAR_INFO_PREAMBLE_THRESHOLD: usize = 4 * 1024; // 4 KiB

/// The API client.
#[derive(Debug, Clone)]
pub struct Client {
    /// Base endpoint of the server.
    endpoint: Url,
    /// An initialized HTTP client.
    client: HttpClient,
}

impl Client {
    pub fn from_server_config(config: ServerConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()?;

        Ok(Self {
            endpoint: Url::parse(&config.endpoint)?,
            client,
        })
    }
    /// Uploads a path.
    pub async fn upload_path<S>(
        &self,
        nar_info: upload_path::Request,
        stream: S,
        force_preamble: bool,
    ) -> Result<Option<upload_path::Response>>
    where
        S: TryStream<Ok = Bytes> + Send + Sync + 'static,
        S::Error: Into<Box<dyn StdError + Send + Sync>> + Send + Sync,
    {
        let endpoint = self.endpoint.join("_api/v1/upload-path")?;
        let upload_info_json = serde_json::to_string(&nar_info)?;

        let mut req = self
            .client
            .put(endpoint)
            .header(reqwest::header::USER_AGENT, HeaderValue::from_str(USER_AGENT)?);

        if force_preamble || upload_info_json.len() >= NAR_INFO_PREAMBLE_THRESHOLD {
            let preamble = Bytes::from(upload_info_json);
            let preamble_len = preamble.len();
            let preamble_stream = stream::once(future::ok(preamble));

            let chained = preamble_stream.chain(stream.into_stream());
            req = req
                .header(header::NAR_INFO_PREAMBLE_SIZE, preamble_len)
                .body(Body::wrap_stream(chained));
        } else {
            req = req
                .header(header::NAR_INFO, HeaderValue::from_str(&upload_info_json)?)
                .body(Body::wrap_stream(stream));
        }

        let res = req.send().await?;

        if res.status().is_success() {
            match res.json().await {
                Ok(r) => Ok(Some(r)),
                Err(_) => Ok(None),
            }
        } else {
            let api_error = Error::try_from_response(res).await?;
            Err(api_error.into())
        }
    }
}
