use serde::{Serialize, Deserialize};

/// Configuration of a cache.
///
/// Specifying `None` means using the default value or
/// keeping the current value.
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheConfig {
    /// The Nix binary cache endpoint of the cache.
    ///
    /// This is the endpoint that should be added to `nix.conf`.
    /// This is read-only and may not be available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substituter_endpoint: Option<String>,

    /// The Attic API endpoint.
    ///
    /// This is read-only and may not be available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_endpoint: Option<String>,

    /// The public key of the cache, in the canonical format used by Nix.
    ///
    /// This is read-only and may not be available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,

    /// Whether the cache is public or not.
    ///
    /// Anonymous clients are implicitly granted the "pull"
    /// permission to public caches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_public: Option<bool>,

    /// The Nix store path this binary cache uses.
    ///
    /// This is usually `/nix/store`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_dir: Option<String>,

    /// The priority of the binary cache.
    ///
    /// A lower number denotes a higher priority.
    /// <https://cache.nixos.org> has a priority of 40.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,

    /// A list of signing key names of upstream caches.
    ///
    /// The list serves as a hint to clients to avoid uploading
    /// store paths signed with such keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_cache_key_names: Option<Vec<String>>,

    /// The retention period of the cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_period: Option<RetentionPeriodConfig>,
}

/// Configuration of retention period.
#[derive(Debug, Serialize, Deserialize)]
pub enum RetentionPeriodConfig {
    /// Use the global default.
    Global,

    /// Specify a retention period in seconds.
    ///
    /// If 0, then time-based garbage collection is disabled.
    Period(u32),
}
