use std::fmt;
use std::error::Error as StdError;
use anyhow::Error as AnyError;
use displaydoc::Display;
use serde::Serialize;
use tracing_error::SpanTrace;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

pub type ServerResult<T> = Result<T, ServerError>;

/// The kind of an error.
#[derive(Debug, Display)]
pub enum ErrorKind {
    /// The server encountered an internal error or misconfiguration.
    InternalServerError,
    /// The URL you requested was not found.
    NotFound,
    /// Storage error: {0}
    StorageError(AnyError),
    /// General request error: {0}
    RequestError(AnyError),
    /// Invalid compression type "{name}".
    InvalidCompressionType { name: String },
    /// Manifest serialization error: {0}
    ManifestSerializationError(super::nix_manifest::Error),
}
impl ErrorKind {
    /// Returns a version of this error for clients.
    fn into_clients(self) -> Self {
        match self {
            Self::InternalServerError => self,
            Self::NotFound => self,
            Self::StorageError(_) => Self::InternalServerError,
            Self::RequestError(_) => self,
            Self::InvalidCompressionType { .. } => self,
            Self::ManifestSerializationError(_) => Self::InternalServerError,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::InternalServerError => "InternalServerError",
            Self::NotFound => "NotFound",
            Self::StorageError(_) => "StorageError",
            Self::RequestError(_) => "RequestError",
            Self::InvalidCompressionType { .. } => "InvalidCompressionType",
            Self::ManifestSerializationError(_) => "ManifestSerializationError",
        }
    }
    fn http_status_code(&self) -> StatusCode {
        match self {
            Self::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RequestError(_) => StatusCode::BAD_REQUEST,
            Self::InvalidCompressionType { .. } => StatusCode::BAD_REQUEST,
            Self::ManifestSerializationError(_) => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Serialize)]
pub struct ErrorResponse {
    code: u16,
    error: String,
    message: String,
}

/// A server error.
#[derive(Debug)]
pub struct ServerError {
    /// The kind of the error.
    kind: ErrorKind,
    /// Context of where the error occurred.
    context: SpanTrace,
}
impl ServerError {
    pub fn storage_error(error: impl StdError + Send + Sync + 'static) -> Self {
        ErrorKind::StorageError(AnyError::new(error)).into()
    }
    pub fn request_error(error: impl StdError + Send + Sync + 'static) -> Self {
        ErrorKind::RequestError(AnyError::new(error)).into()
    }
}
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.kind)?;
        self.context.fmt(f)?;
        Ok(())
    }
}
impl StdError for ServerError {}
impl From<ErrorKind> for ServerError {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind,
            context: SpanTrace::capture(),
        }
    }
}
impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        if matches!(
            self.kind,
            ErrorKind::StorageError(_) | ErrorKind::RequestError(_)
            )
        {
            tracing::error!("{}", self);
        }

        let sanitized = self.kind.into_clients();

        let status_code = sanitized.http_status_code();
        let error_response = ErrorResponse {
            code: status_code.as_u16(),
            message: sanitized.to_string(),
            error: sanitized.name().to_string(),
        };

        (status_code, Json(error_response)).into_response()
    }
}
