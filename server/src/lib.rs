pub mod api;
pub mod config;
pub mod error;
pub mod storage;
pub mod chunking;
pub mod narinfo;
pub mod nix_manifest;
pub mod stream;
pub mod access;
pub mod finally;

use anyhow::Result;
use std::sync::Arc;
use axum::{routing::get, Server, Router, extract::Extension, http::Uri};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

use crate::config::{Config, StorageConfig};
use crate::error::{ErrorKind, ServerResult};
use crate::storage::{StorageBackend, local::LocalBackend};
use crate::access::RequireAuth;

/// Global server state.
#[derive(Debug, Clone)]
pub struct State {
    /// The Attic Server configuration.
    config: Config,
    /// Handle to the storage backend.
    storage: Arc<Box<dyn StorageBackend>>,
}
impl State {
    async fn new(config: Config) -> Result<Arc<Self>> {
        let storage = match &config.storage {
            StorageConfig::Local(local_config) => {
                let local = LocalBackend::new(local_config.clone()).await?;
                let boxed: Box<dyn StorageBackend> = Box::new(local);
                Arc::new(boxed)
            }
        };

        Ok(Arc::new(Self {
            config,
            storage,
        }))
    }
    /// Returns a handle to the storage backend.
    fn storage(&self) -> Arc<Box<dyn StorageBackend>> {
        Arc::clone(&self.storage)
    }
}

/// Runs the API server.
pub async fn run_api_server(config: Config) -> Result<()> {
    tracing::info!("Starting API server...");

    if config.token_hs256_secret.is_none() {
        tracing::warn!("Authentication is disabled, anyone will be able to access this cache.");
    }

    let listen = config.listen;
    let state = State::new(config).await?;

    let rest = Router::new()
        .merge(api::router())
        .fallback(fallback)
        .layer(axum::middleware::from_extractor_with_state::<RequireAuth, Arc<State>>(Arc::clone(&state)))
        .route("/", get(home))
        .layer(Extension(state))
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new());

    tracing::info!("Listening on {:?}...", listen);
    Server::bind(&listen).serve(rest.into_make_service()).await?;

    Ok(())
}

/// The home route.
async fn home() -> String {
    format!("Nixcache {}", env!("CARGO_PKG_VERSION"))
}

/// The fallback route.
#[axum_macros::debug_handler]
async fn fallback(_: Uri) -> ServerResult<()> {
    Err(ErrorKind::NotFound.into())
}
