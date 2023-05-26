pub mod api;
pub mod config;
pub mod error;

use anyhow::Result;
use std::sync::Arc;
use axum::{Server, Router, extract::Extension, http::Uri};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::error::{ErrorKind, ServerResult};

/// Global server state.
#[derive(Debug)]
pub struct State {
    /// The Attic Server configuration.
    config: Config,
}
impl State {
    fn new(config: Config) -> Self {
        Self { config }
    }
}

/// Runs the API server.
pub async fn run_api_server(config: Config) -> Result<()> {
    eprintln!("Starting API server...");

    let listen = config.listen;
    let state = State::new(config);

    let rest = Router::new()
        .merge(api::router())
        .fallback(fallback)
        .layer(Extension(Arc::new(state)))
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new());

    eprintln!("Listening on {:?}...", listen);
    Server::bind(&listen).serve(rest.into_make_service()).await?;

    Ok(())
}

/// The fallback route.
#[axum_macros::debug_handler]
async fn fallback(_: Uri) -> ServerResult<()> {
    Err(ErrorKind::NotFound.into())
}
