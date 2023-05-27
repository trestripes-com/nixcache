pub mod v1;

use axum::{routing::get, Router};

async fn home() -> String {
    format!("Nix cache {}", env!("CARGO_PKG_VERSION"))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(home))
        // .merge(binary_cache::get_router())
        .nest("/_api", Router::new().nest("/v1", v1::router()))
}
