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

//! Blob reference (`$blob` sentinel) for representing blobified data in JSON values.
//!
//! A blob ref is a JSON object with a `$blob` key that indicates "this value is stored
//! in the blob store". This is a runtime data convention, distinct from workflow-definition
//! `ValueExpr` (`$step`, `$input`, `$variable`).
//!
//! ```json
//! {"$blob": "<sha256hex>", "blobType": "data", "size": 12345}
//! ```

use serde::{Deserialize, Serialize};

use crate::blob::{BlobId, BlobType};

/// The sentinel key used to identify blob references in JSON values.
pub const BLOB_REF_KEY: &str = "$blob";

/// A reference to a blob stored in the blob store.
///
/// Appears in JSON data as `{"$blob": "<id>", "blobType": "data", "size": 12345}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobRef {
    /// The blob ID (SHA-256 hash).
    #[serde(rename = "$blob")]
    pub blob_id: BlobId,

    /// The type of blob (data, binary, flow). Defaults to data.
    #[serde(default, skip_serializing_if = "is_default_blob_type")]
    pub blob_type: BlobType,

    /// Byte size of the original data before blobification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

fn is_default_blob_type(t: &BlobType) -> bool {
    *t == BlobType::Data
}

impl BlobRef {
    /// Create a new blob ref.
    pub fn new(blob_id: BlobId, blob_type: BlobType, size: Option<u64>) -> Self {
        Self {
            blob_id,
            blob_type,
            size,
        }
    }

    /// Try to parse a JSON value as a blob ref.
    ///
    /// Returns `Some(BlobRef)` if the value is an object with a `$blob` key,
    /// `None` otherwise.
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        let obj = value.as_object()?;
        if !obj.contains_key(BLOB_REF_KEY) {
            return None;
        }
        let parsed: Self = serde_json::from_value(value.clone()).ok()?;
        // Validate the blob ID (serde Deserialize for BlobId doesn't validate)
        BlobId::new(parsed.blob_id.as_str().to_string()).ok()?;
        Some(parsed)
    }

    /// Convert this blob ref to a JSON value.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("BlobRef serialization should not fail")
    }
}

/// Quick check whether a JSON value is a blob ref (has a `$blob` key).
pub fn is_blob_ref(value: &serde_json::Value) -> bool {
    value
        .as_object()
        .is_some_and(|obj| obj.contains_key(BLOB_REF_KEY))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_blob_ref_round_trip() {
        let blob_id = BlobId::new("a".repeat(64)).unwrap();
        let blob_ref = BlobRef::new(blob_id.clone(), BlobType::Data, Some(1024));
        let value = blob_ref.to_value();
        let parsed = BlobRef::from_value(&value).expect("should parse blob ref");
        assert_eq!(parsed, blob_ref);
    }

    #[test]
    fn test_blob_ref_minimal() {
        let blob_id = BlobId::new("b".repeat(64)).unwrap();
        let blob_ref = BlobRef::new(blob_id.clone(), BlobType::Data, None);
        let value = blob_ref.to_value();

        // Default blob_type and None size should be omitted
        let obj = value.as_object().unwrap();
        assert!(obj.contains_key("$blob"));
        assert!(!obj.contains_key("blobType"));
        assert!(!obj.contains_key("size"));
    }

    #[test]
    fn test_blob_ref_binary_type() {
        let blob_id = BlobId::new("c".repeat(64)).unwrap();
        let blob_ref = BlobRef::new(blob_id.clone(), BlobType::Binary, Some(2048));
        let value = blob_ref.to_value();

        let obj = value.as_object().unwrap();
        assert_eq!(obj["blobType"], "binary");
        assert_eq!(obj["size"], 2048);

        let parsed = BlobRef::from_value(&value).unwrap();
        assert_eq!(parsed.blob_type, BlobType::Binary);
    }

    #[test]
    fn test_is_blob_ref() {
        let blob_value = json!({"$blob": "a".repeat(64)});
        assert!(is_blob_ref(&blob_value));

        let not_blob = json!({"key": "value"});
        assert!(!is_blob_ref(&not_blob));

        let string = json!("not a blob");
        assert!(!is_blob_ref(&string));
    }

    #[test]
    fn test_from_value_non_blob() {
        assert!(BlobRef::from_value(&json!({"key": "value"})).is_none());
        assert!(BlobRef::from_value(&json!("string")).is_none());
        assert!(BlobRef::from_value(&json!(42)).is_none());
        assert!(BlobRef::from_value(&json!(null)).is_none());
    }

    #[test]
    fn test_from_value_invalid_blob_id() {
        // $blob key present but value is not a valid blob ID
        let value = json!({"$blob": "too_short"});
        assert!(BlobRef::from_value(&value).is_none());
    }
}
