use anyhow::Result;
use displaydoc::Display;
use serde::{Serialize, Deserialize, de};
use lazy_static::lazy_static;
use regex::Regex;

/// Length of the hash in a store path.
pub const STORE_PATH_HASH_LEN: usize = 32;

/// Regex that matches a store path hash, without anchors.
pub const STORE_PATH_HASH_REGEX_FRAGMENT: &str = "[0123456789abcdfghijklmnpqrsvwxyz]{32}";

lazy_static! {
    /// Regex for a valid store path hash.
    ///
    /// This is the path portion of a base name.
    static ref STORE_PATH_HASH_REGEX: Regex = {
        Regex::new(&format!("^{}$", STORE_PATH_HASH_REGEX_FRAGMENT)).unwrap()
    };

    /// Regex for a valid store base name.
    ///
    /// A base name consists of two parts: A hash and a human-readable
    /// label/name. The format of the hash is described in `StorePathHash`.
    ///
    /// The human-readable name can only contain the following characters:
    ///
    /// - A-Za-z0-9
    /// - `+-._?=`
    ///
    /// See the Nix implementation in `src/libstore/path.cc`.
    static ref STORE_BASE_NAME_REGEX: Regex = {
        Regex::new(r"^[0123456789abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+-._?=]+$").unwrap()
    };
}

#[derive(Debug, Display)]
pub enum Error {
    /// Invalid store path hash "{hash}": {reason}
    InvalidStorePathHash { hash: String, reason: &'static str },
}
impl std::error::Error for Error {}

/// A fixed-length store path hash.
///
/// For example, for `/nix/store/ia70ss13m22znbl8khrf2hq72qmh5drr-ruby-2.7.5`,
/// this would be `ia70ss13m22znbl8khrf2hq72qmh5drr`.
///
/// It must contain exactly 32 "base-32 characters". Nix's special scheme
/// include the following valid characters: "0123456789abcdfghijklmnpqrsvwxyz"
/// ('e', 'o', 'u', 't' are banned).
///
/// Examples of invalid store path hashes:
///
/// - "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
/// - "IA70SS13M22ZNBL8KHRF2HQ72QMH5DRR"
/// - "whatevenisthisthing"
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize)]
pub struct StorePathHash(String);

impl StorePathHash {
    /// Creates a store path hash from a string.
    pub fn new(hash: String) -> Result<Self> {
        if hash.as_bytes().len() != STORE_PATH_HASH_LEN {
            return Err(Error::InvalidStorePathHash {
                hash,
                reason: "Hash is of invalid length",
            }.into());
        }

        if !STORE_PATH_HASH_REGEX.is_match(&hash) {
            return Err(Error::InvalidStorePathHash {
                hash,
                reason: "Hash is of invalid format",
            }.into());
        }

        Ok(Self(hash))
    }

    /// Creates a store path hash from a string, without checking its validity.
    ///
    /// # Safety
    ///
    /// The caller must make sure that it is of expected length and format.
    #[allow(unsafe_code)]
    pub unsafe fn new_unchecked(hash: String) -> Self {
        Self(hash)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl<'de> Deserialize<'de> for StorePathHash {
    /// Deserializes a potentially-invalid store path hash.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        use de::Error;
        String::deserialize(deserializer)
            .and_then(|s| Self::new(s).map_err(|e| Error::custom(e.to_string())))
    }
}
