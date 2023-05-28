use anyhow::Result;
use std::error::Error as StdError;
use std::fmt;
use serde::Deserialize;
use displaydoc::Display;
use reqwest::{Response, StatusCode};

/// API error.
#[derive(Debug, Display)]
pub enum Error {
    /// {0}
    Structured(StructuredApiError),
    /// HTTP {0}: {1}
    Unstructured(StatusCode, String),
}
impl StdError for Error {}
impl Error {
    pub async fn try_from_response(response: Response) -> Result<Self> {
        let status = response.status();
        let text = response.text().await?;
        match serde_json::from_str(&text) {
            Ok(s) => Ok(Self::Structured(s)),
            Err(_) => Ok(Self::Unstructured(status, text)),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StructuredApiError {
    #[allow(dead_code)]
    code: u16,
    error: String,
    message: String,
}
impl fmt::Display for StructuredApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.error, self.message)
    }
}

