// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use std::fmt;

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::workflow::{Flow, ValueRef};
use std::sync::Arc;

/// Typed blob value that keeps the data in a convenient form based on type.
#[derive(Debug, Clone, PartialEq)]
pub enum BlobValue {
    /// Generic JSON data
    Json(ValueRef),
    /// A workflow/flow definition  
    Flow(Arc<Flow>),
}

impl BlobValue {
    /// Get the blob type for this value
    pub fn blob_type(&self) -> BlobType {
        match self {
            BlobValue::Json(_) => BlobType::Data,
            BlobValue::Flow(_) => BlobType::Flow,
        }
    }

    /// Convert to ValueRef for serialization
    pub fn to_value_ref(&self) -> ValueRef {
        match self {
            BlobValue::Json(value_ref) => value_ref.clone(),
            BlobValue::Flow(flow) => ValueRef::new(serde_json::to_value(flow.as_ref()).unwrap()),
        }
    }

    /// Try to create a BlobValue from ValueRef and type
    pub fn from_value_ref(data: ValueRef, blob_type: BlobType) -> Result<Self, BlobValueError> {
        match blob_type {
            BlobType::Data => Ok(BlobValue::Json(data)),
            BlobType::Flow => {
                let flow: Flow = serde_json::from_value(data.as_ref().clone())
                    .map_err(|_| BlobValueError::InvalidFlowData)?;
                Ok(BlobValue::Flow(Arc::new(flow)))
            }
        }
    }
}

/// Error type for BlobValue operations
#[derive(Debug, thiserror::Error)]
pub enum BlobValueError {
    #[error("Invalid flow data - could not deserialize")]
    InvalidFlowData,
}

/// Structured blob data containing both the content and metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct BlobData {
    /// The blob content in typed form
    pub value: BlobValue,
    /// The blob ID (for convenience)
    pub blob_id: BlobId,
}

impl BlobData {
    /// Create new blob data with typed value
    pub fn new(value: BlobValue, blob_id: BlobId) -> Self {
        Self { value, blob_id }
    }

    /// Create blob data from ValueRef and type
    pub fn from_value_ref(
        data: ValueRef,
        blob_type: BlobType,
        blob_id: BlobId,
    ) -> Result<Self, BlobValueError> {
        let value = BlobValue::from_value_ref(data, blob_type)?;
        Ok(Self::new(value, blob_id))
    }

    /// Get the blob type
    pub fn blob_type(&self) -> BlobType {
        self.value.blob_type()
    }

    /// Get the data as ValueRef for backward compatibility
    pub fn data(&self) -> ValueRef {
        self.value.to_value_ref()
    }

    /// Get a reference to the typed value
    pub fn as_flow(&self) -> Option<&Arc<Flow>> {
        match &self.value {
            BlobValue::Flow(flow) => Some(flow),
            _ => None,
        }
    }

    /// Get a reference to the JSON data
    pub fn as_json(&self) -> Option<&ValueRef> {
        match &self.value {
            BlobValue::Json(data) => Some(data),
            _ => None,
        }
    }
}

/// Type of blob stored in the blob store.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlobType {
    /// A workflow/flow definition
    Flow,
    /// Generic data blob
    #[default]
    Data,
}

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

/// A SHA-256 hash of the blob content, represented as a hexadecimal string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
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

    /// Generate a content-based blob ID from a Flow using SHA-256.
    pub fn from_flow(flow: &crate::workflow::Flow) -> error_stack::Result<Self, BlobIdError> {
        let flow_data =
            ValueRef::new(serde_json::to_value(flow).change_context(BlobIdError::SerializeFailed)?);
        Self::from_content(&flow_data)
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
