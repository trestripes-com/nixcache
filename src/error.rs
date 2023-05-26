use std::fmt;
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
    /// The URL you requested was not found.
    NotFound,
}
impl ErrorKind {
    /// Returns a version of this error for clients.
    fn into_clients(self) -> Self {
        match self {
            Self::NotFound => self,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::NotFound => "NotFound",
        }
    }
    fn http_status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
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
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.kind)?;
        self.context.fmt(f)?;
        Ok(())
    }
}
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
        tracing::error!("{}", self);

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
