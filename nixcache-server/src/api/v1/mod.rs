pub mod upload_path;

use axum::{Router, routing::put};

pub fn router() -> Router {
    Router::new()
        .route(
            "/upload-path",
            put(upload_path::upload_path),
        )
}
