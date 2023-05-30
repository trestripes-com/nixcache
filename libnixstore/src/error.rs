use std::path::PathBuf;
use displaydoc::Display;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Display)]
pub enum Error {
    /// Invalid store path {path:?}: {reason}
    InvalidStorePath { path: PathBuf, reason: &'static str },

    /// Invalid store path base name {base_name:?}: {reason}
    InvalidStorePathName {
        base_name: PathBuf,
        reason: &'static str,
    },

    /// Invalid store path hash "{hash}": {reason}
    InvalidStorePathHash { hash: String, reason: &'static str },

    /// Unknown C++ exception: {exception}.
    CxxError { exception: String },

    /// I/O error: {error}.
    IoError { error: std::io::Error },

    /// Hashing error: {0}
    HashError(crate::hash::Error),
}

impl std::error::Error for Error {}

impl From<cxx::Exception> for Error {
    fn from(exception: cxx::Exception) -> Self {
        Self::CxxError {
            exception: exception.what().to_string(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::IoError { error }
    }
}

impl From<crate::hash::Error> for Error {
    fn from(error: crate::hash::Error) -> Self {
        Self::HashError(error)
    }
}
