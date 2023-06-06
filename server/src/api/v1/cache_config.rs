use std::sync::Arc;
use axum::extract::{Extension, Json};
use tracing::instrument;

use common::v1::cache_config::{CacheConfig, RetentionPeriodConfig};
use crate::error::ServerResult;
use crate::State;

#[instrument(skip_all)]
pub async fn get(
    Extension(state): Extension<Arc<State>>,
) -> ServerResult<Json<CacheConfig>> {
    let public_key = state.config.keypair.export_public_key();
    let retention_period_config = RetentionPeriodConfig::Global;

    Ok(Json(CacheConfig {
        substituter_endpoint: None,
        api_endpoint: None,
        public_key: Some(public_key),
        is_public: Some(false),
        store_dir: Some(super::CACHE_STOREDIR.to_string()),
        priority: Some(super::CACHE_PRIORITY),
        upstream_cache_key_names: None,
        retention_period: Some(retention_period_config),
    }))
}
