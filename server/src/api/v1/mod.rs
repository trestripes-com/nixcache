pub mod upload_path;

use axum::{Router, routing::{get, post, put, patch, delete}};

pub fn router() -> Router {
    Router::new()
        .route(
            "/upload-path",
            put(upload_path::upload_path),
        )
}
