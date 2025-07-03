// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::borrow::Cow;
use std::fmt;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::workflow::ValueRef;

#[derive(Debug, thiserror::Error)]
pub enum BlobIdError {
    #[error("Invalid blob ID length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },

    #[error("Invalid characters in blob ID")]
    InvalidCharacters,

    #[error("Failed to serialize data")]
    SerializeFailed,

    #[error("Blob not found: {blob_id}")]
    BlobNotFound { blob_id: String },
}

/// A type-safe wrapper for blob identifiers.
///
/// Blob IDs are SHA-256 hashes of the content, providing deterministic
/// identification and automatic deduplication.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct BlobId(String);

impl schemars::JsonSchema for BlobId {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("BlobId")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "title": "Blob ID",
            "description": "A SHA-256 hash of the blob content, represented as a hexadecimal string.",
            "type": "string",
        })
    }
}

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
