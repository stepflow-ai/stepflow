use std::fmt;

use error_stack::ResultExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::workflow::ValueRef;

/// A type-safe wrapper for blob identifiers.
///
/// Blob IDs are SHA-256 hashes of the content, providing deterministic
/// identification and automatic deduplication.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct BlobId(String);

impl BlobId {
    /// Create a new BlobId from a hex-encoded hash string.
    ///
    /// This validates that the string is a valid SHA-256 hash (64 hex characters).
    pub fn new(hash: String) -> error_stack::Result<Self, BlobIdError> {
        error_stack::ensure!(
            hash.len() == 64,
            BlobIdError::InvalidLength {
                expected: 64,
                actual: hash.len(),
            }
        );

        error_stack::ensure!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            BlobIdError::InvalidCharacters
        );

        Ok(BlobId(hash))
    }

    /// Generate a content-based blob ID from JSON data using SHA-256.
    pub fn from_content(data: &ValueRef) -> error_stack::Result<Self, BlobIdError> {
        let mut hasher = Sha256::new();

        // Serialize to deterministic JSON bytes for hashing
        serde_json::to_writer(&mut hasher, data.as_ref())
            .change_context(BlobIdError::SerializeFailed)?;

        let hash = hex::encode(hasher.finalize());
        Self::new(hash)
    }

    /// Get the inner hash string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for BlobId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Errors that can occur when creating a BlobId.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BlobIdError {
    #[error("Invalid blob ID length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("Invalid blob ID: contains non-hex characters")]
    InvalidCharacters,
    #[error("Failed to serialize content for blob ID")]
    SerializeFailed,
}
