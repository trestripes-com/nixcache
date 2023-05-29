pub mod v1;

use axum::{routing::get, Router};
use serde::{Serialize, Deserialize};

use nixcache_common::Hash;
use crate::config::CompressionConfig;

#[derive(Serialize, Deserialize)]
pub struct UploadedChunk {
    file_hash: Hash,
    file_size: usize,
    compression: CompressionConfig,
}
#[derive(Serialize, Deserialize)]
pub struct UploadedNar {
    nar_size: usize,
    chunks: Vec<UploadedChunk>,
}

async fn home() -> String {
    format!("Nix cache {}", env!("CARGO_PKG_VERSION"))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(home))
        // .merge(binary_cache::get_router())
        .nest("/_api", Router::new().nest("/v1", v1::router()))
}
