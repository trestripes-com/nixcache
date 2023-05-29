//! Nix Binary Cache server.
//!
//! This module implements the Nix Binary Cache API.
//!
//! The implementation is based on the specifications at <https://github.com/fzakaria/nix-http-binary-cache-api-spec>.

use anyhow::anyhow;
use std::path::PathBuf;
use std::sync::Arc;
use axum::{
    body::StreamBody,
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, head},
    Router,
};
use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use futures::stream::BoxStream;
use tracing::instrument;

use nixbase32::from_nix_base32;
use common::{mime, Hash, StorePathHash};
use crate::error::{ErrorKind, ServerResult, ServerError};
use crate::{nix_manifest, State, narinfo::NarInfo};
use crate::storage::Download;
use crate::api::UploadedNar;

/// Nix cache information.
///
/// An example of a correct response is as follows:
///
/// ```text
/// StoreDir: /nix/store
/// WantMassQuery: 1
/// Priority: 40
/// ```
#[derive(Debug, Clone, Serialize)]
struct NixCacheInfo {
    /// Whether this binary cache supports bulk queries.
    #[serde(rename = "WantMassQuery")]
    want_mass_query: bool,
    /// The Nix store path this binary cache uses.
    #[serde(rename = "StoreDir")]
    store_dir: PathBuf,
    /// The priority of the binary cache.
    ///
    /// A lower number denotes a higher priority.
    /// <https://cache.nixos.org> has a priority of 40.
    #[serde(rename = "Priority")]
    priority: i32,
}
impl IntoResponse for NixCacheInfo {
    fn into_response(self) -> Response {
        match nix_manifest::to_string(&self) {
            Ok(body) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime::NIX_CACHE_INFO)
                .body(body)
                .unwrap()
                .into_response(),
            Err(e) => e.into_response(),
        }
    }
}

/// Gets information on a cache.
#[instrument(skip_all)]
async fn get_nix_cache_info() -> ServerResult<NixCacheInfo> {
    let info = NixCacheInfo {
        want_mass_query: true,
        store_dir: "/nix/store".into(),
        priority: 80,
    };
    Ok(info)
}

/// Gets various information on a store path hash.
///
/// `/:path`, which may be one of
/// - GET  `/{storePathHash}.narinfo`
/// - HEAD `/{storePathHash}.narinfo`
/// - GET  `/{storePathHash}.ls`      (NOT IMPLEMENTED)
#[instrument(skip_all, fields(path))]
#[axum_macros::debug_handler]
async fn get_store_path_info(
    Extension(state): Extension<Arc<State>>,
    Path(path): Path<String>,
) -> ServerResult<NarInfo> {
    let components: Vec<&str> = path.splitn(2, '.').collect();
    if components.len() != 2 {
        return Err(ErrorKind::NotFound.into());
    }
    if components[1] != "narinfo" {
        return Err(ErrorKind::NotFound.into());
    }

    let store_path_hash = StorePathHash::new(components[0].to_string())
        .map_err(|e| ErrorKind::RequestError(anyhow!(
            "Could not parse store path hash : {}", e
        )))?;

    tracing::debug!("Received request for {}.narinfo", store_path_hash.as_str());

    // Get NAR
    let backend = state.storage();
    let nar = backend
        .download_nar(store_path_hash.to_string())
        .await?;

    let narinfo = match nar {
        Download::AsyncRead(mut stream) => {
            let mut nar = Vec::new();
            stream.read_to_end(&mut nar).await
                .map_err(ServerError::storage_error)?;
            let nar: UploadedNar = serde_json::from_slice(&nar)
                .map_err(ServerError::storage_error)?;
            nar.into_narinfo(&store_path_hash)
        },
    };

    // if narinfo.signature().is_none() {
    //     narinfo.sign(&keypair);
    // }

    Ok(narinfo)
}

pub fn router() -> Router {
    Router::new()
        .route("/nix-cache-info", get(get_nix_cache_info))
        .route("/:path", get(get_store_path_info))
        .route("/:path", head(get_store_path_info))
        // .route("/nar/:path", get(get_nar))
}
