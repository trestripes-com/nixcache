use axum::{routing::get, Router};

async fn home() -> &'static str {
    "Nix cache"
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(home))
        // .merge(binary_cache::get_router())
        // .merge(v1::get_router())
}
