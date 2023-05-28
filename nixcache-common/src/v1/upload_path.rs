use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DefaultOnError};

use crate::{StorePathHash, Hash};

/// NAR information associated with a upload.
///
/// There are two ways for the client to supply the NAR information:
///
/// 1. At the beginning of the PUT body. The `X-Attic-Nar-Info-Preamble-Size`
///    header must be set to the size of the JSON.
/// 2. Through the `X-Attic-Nar-Info` header.
///
/// The client is advised to use the first method if the serialized
/// JSON is large (>4K).
///
/// Regardless of client compression, the server will always decompress
/// the NAR to validate the NAR hash before applying the server-configured
/// compression again.
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    /// The hash portion of the store path.
    pub store_path_hash: StorePathHash,

    /// The full store path being cached, including the store directory.
    pub store_path: String,

    /// Other store paths this object directly refereces.
    pub references: Vec<String>,

    /// The system this derivation is built for.
    pub system: Option<String>,

    /// The derivation that produced this object.
    pub deriver: Option<String>,

    /// The signatures of this object.
    pub sigs: Vec<String>,

    /// The CA field of this object.
    pub ca: Option<String>,

    /// The hash of the NAR.
    ///
    /// It must begin with `sha256:` with the SHA-256 hash in the
    /// hexadecimal format (64 hex characters).
    ///
    /// This is informational and the server always validates the supplied
    /// hash.
    pub nar_hash: Hash,

    /// The size of the NAR.
    pub nar_size: usize,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    #[serde_as(deserialize_as = "DefaultOnError")]
    pub kind: ResponseKind,

    /// The compressed size of the NAR, in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size: Option<usize>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ResponseKind {
    /// The path was uploaded.
    ///
    /// This is purely informational and servers may return
    /// this variant even when the NAR is deduplicated.
    Uploaded,

    /// The path was globally deduplicated.
    ///
    /// The exact semantics of what counts as deduplicated
    /// is opaque to the client.
    Deduplicated,
}
impl Default for ResponseKind {
    fn default() -> Self {
        Self::Uploaded
    }
}
