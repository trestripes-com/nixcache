pub mod api;
pub mod config;
pub mod error;
pub mod storage;
pub mod chunking;
pub mod narinfo;
pub mod nix_manifest;

use anyhow::Result;
use std::sync::Arc;
use axum::{Server, Router, extract::Extension, http::Uri};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

use crate::config::{Config, StorageConfig};
use crate::error::{ErrorKind, ServerResult};
use crate::storage::{StorageBackend, local::LocalBackend};

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
    eprintln!("Starting API server...");

    let listen = config.listen;
    let state = State::new(config).await?;

    let rest = Router::new()
        .merge(api::router())
        .fallback(fallback)
        .layer(Extension(state))
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
