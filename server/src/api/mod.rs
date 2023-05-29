pub mod binary_cache;
pub mod v1;

use std::path::PathBuf;
use axum::{routing::get, Router};
use serde::{Serialize, Deserialize};
use serde_with::serde_as;

use common::{Hash, StorePathHash};
use crate::config::CompressionConfig;
use crate::narinfo::{self, NarInfo};
use crate::nix_manifest::SpaceDelimitedList;

#[derive(Serialize, Deserialize)]
pub struct UploadedChunk {
    file_hash: Hash,
    file_size: usize,
    compression: CompressionConfig,
}
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct UploadedNar {
    /// The full store path being cached, including the store directory.
    ///
    /// Part of the fingerprint.
    ///
    /// Example: `/nix/store/p4pclmv1gyja5kzc26npqpia1qqxrf0l-ruby-2.7.3`.
    #[serde(rename = "StorePath")]
    pub store_path: PathBuf,

    /// The hash of the NAR archive.
    ///
    /// Part of the fingerprint.
    #[serde(rename = "NarHash")]
    pub nar_hash: Hash,
    /// The size of the NAR archive.
    ///
    /// Part of the fingerprint.
    #[serde(rename = "NarSize")]
    pub nar_size: usize,

    /// Other store paths this object directly refereces.
    ///
    /// This only includes the base paths, not the store directory itself.
    ///
    /// Part of the fingerprint.
    ///
    /// Example element: `j5p0j1w27aqdzncpw73k95byvhh5prw2-glibc-2.33-47`
    #[serde(rename = "References")]
    #[serde_as(as = "SpaceDelimitedList")]
    pub references: Vec<String>,

    /// The system this derivation is built for.
    #[serde(rename = "System")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,

    /// The content address of the object.
    #[serde(rename = "CA")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca: Option<String>,

    chunks: Vec<UploadedChunk>,
}
impl UploadedNar {
    fn into_narinfo(self, store_path_hash: &StorePathHash) -> NarInfo {
        NarInfo {
            store_path: PathBuf::from(self.store_path),
            url: format!("nar/{}.nar", store_path_hash.as_str()),
            compression: narinfo::Compression::None,
            file_hash: None,
            file_size: None,
            nar_hash: self.nar_hash,
            nar_size: self.nar_size,
            system: self.system,
            references: self.references,
            deriver: None,
            signature: None,
            ca: self.ca,
        }
    }
}

async fn home() -> String {
    format!("Nix cache {}", env!("CARGO_PKG_VERSION"))
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(home))
        .merge(binary_cache::router())
        .nest("/_api", Router::new().nest("/v1", v1::router()))
}
