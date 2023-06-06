pub mod upload_path;
pub mod cache_config;

use axum::Router;
use axum::routing::{get, put};

pub const CACHE_PRIORITY: i32 = 80;
pub const CACHE_STOREDIR: &str = "/nix/store";

pub fn router() -> Router {
    Router::new()
        .route("/upload-path", put(upload_path::upload_path))
        .route("/cache-config", get(cache_config::get))
}
