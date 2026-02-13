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

//! Utility functions for blobifying large values and resolving blob references.
//!
//! These functions operate on JSON values, replacing large top-level fields with
//! blob refs (blobification) and replacing blob refs with their inline data (resolution).

use error_stack::ResultExt as _;
use serde_json::Value;
use stepflow_core::BlobId;
use stepflow_core::blob::BlobType;
use stepflow_core::blob_ref::{BlobRef, is_blob_ref};
use stepflow_core::workflow::ValueRef;

use crate::{BlobStore, StateError};

/// Replace large top-level fields in an input object with blob refs.
///
/// Checks each top-level field of the input object individually against the threshold.
/// Fields whose JSON serialization exceeds the threshold are stored as blobs and
/// replaced with `{"$blob": "<id>", "blobType": "data", "size": <bytes>}`.
///
/// Returns the (possibly modified) input and a list of blob IDs that were created.
///
/// If `threshold` is 0 or the input is not an object, it is returned unchanged.
pub async fn blobify_inputs(
    input: Value,
    threshold: usize,
    blob_store: &dyn BlobStore,
) -> error_stack::Result<(Value, Vec<BlobId>), StateError> {
    if threshold == 0 {
        return Ok((input, vec![]));
    }

    let Value::Object(mut map) = input else {
        return Ok((input, vec![]));
    };

    let mut created_blob_ids = Vec::new();

    for (key, value) in map.iter_mut() {
        // Skip values that are already blob refs
        if is_blob_ref(value) {
            continue;
        }

        // serde_json::to_vec is infallible for serde_json::Value
        let serialized =
            serde_json::to_vec(value).expect("serde_json::Value serialization is infallible");
        let size = serialized.len();

        if size > threshold {
            let value_ref = ValueRef::new(value.clone());
            let blob_id = blob_store.put_blob(value_ref, BlobType::Data).await?;
            log::debug!(
                "Blobified field {:?} ({} bytes) -> {}",
                key,
                size,
                &blob_id.to_string()[..12]
            );
            let blob_ref = BlobRef::new(blob_id.clone(), BlobType::Data, Some(size as u64));
            *value = blob_ref.to_value();
            created_blob_ids.push(blob_id);
        }
    }

    Ok((Value::Object(map), created_blob_ids))
}

/// Recursively replace blob refs in a JSON value with their inline data.
///
/// All blob refs at the current level are fetched in parallel, then substituted.
/// If the resolved data contains further blob refs, another parallel round is
/// performed (up to [`MAX_RESOLVE_DEPTH`] rounds).
pub async fn resolve_blob_refs(
    value: Value,
    blob_store: &dyn BlobStore,
) -> error_stack::Result<Value, StateError> {
    resolve_blob_refs_inner(value, blob_store, 0).await
}

/// Maximum recursion depth for blob ref resolution to prevent infinite loops.
const MAX_RESOLVE_DEPTH: usize = 8;

/// Collect all blob ref IDs from a JSON value tree.
fn collect_blob_ids(value: &Value, ids: &mut std::collections::HashSet<BlobId>) {
    match value {
        Value::Object(_map) => {
            if let Some(blob_ref) = BlobRef::from_value(value) {
                ids.insert(blob_ref.blob_id);
            } else {
                for val in _map.values() {
                    collect_blob_ids(val, ids);
                }
            }
        }
        Value::Array(arr) => {
            for val in arr {
                collect_blob_ids(val, ids);
            }
        }
        _ => {}
    }
}

/// Replace blob refs with pre-fetched data (no I/O).
fn replace_blob_refs(value: Value, resolved: &std::collections::HashMap<BlobId, Value>) -> Value {
    match value {
        Value::Object(ref _map) => {
            if let Some(blob_ref) = BlobRef::from_value(&value) {
                if let Some(data) = resolved.get(&blob_ref.blob_id) {
                    return data.clone();
                }
                return value;
            }
            let Value::Object(map) = value else {
                unreachable!()
            };
            let mut result = serde_json::Map::with_capacity(map.len());
            for (key, val) in map {
                result.insert(key, replace_blob_refs(val, resolved));
            }
            Value::Object(result)
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| replace_blob_refs(v, resolved))
                .collect(),
        ),
        other => other,
    }
}

fn resolve_blob_refs_inner(
    value: Value,
    blob_store: &dyn BlobStore,
    depth: usize,
) -> futures::future::BoxFuture<'_, error_stack::Result<Value, StateError>> {
    Box::pin(async move {
        if depth >= MAX_RESOLVE_DEPTH {
            return Ok(value);
        }

        // Collect all blob IDs in the current tree
        let mut ids = std::collections::HashSet::new();
        collect_blob_ids(&value, &mut ids);
        if ids.is_empty() {
            return Ok(value);
        }

        log::debug!("Resolving {} blob ref(s) in parallel", ids.len());

        // Fetch all blobs in parallel
        let ids_vec: Vec<BlobId> = ids.into_iter().collect();
        let fetches = ids_vec.iter().map(|id| {
            let id = id.clone();
            async move {
                let data = blob_store
                    .get_blob(&id)
                    .await
                    .attach_printable_lazy(|| format!("blob_id={id}"));
                data.map(|d| (id, d))
            }
        });
        let results = futures::future::try_join_all(fetches).await?;
        let resolved: std::collections::HashMap<BlobId, Value> = results
            .into_iter()
            .map(|(id, data)| (id, data.data().as_ref().clone()))
            .collect();

        // Replace refs with fetched data
        let result = replace_blob_refs(value, &resolved);

        // Check if resolved data contains more blob refs
        let mut next_ids = std::collections::HashSet::new();
        collect_blob_ids(&result, &mut next_ids);
        if !next_ids.is_empty() {
            return resolve_blob_refs_inner(result, blob_store, depth + 1).await;
        }
        Ok(result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStateStore;
    use serde_json::json;

    #[tokio::test]
    async fn test_blobify_inputs_below_threshold() {
        let store = InMemoryStateStore::new();
        let input = json!({"small": "value"});
        let (result, ids) = blobify_inputs(input.clone(), 1000, &store).await.unwrap();
        assert_eq!(result, input);
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn test_blobify_inputs_above_threshold() {
        let store = InMemoryStateStore::new();
        let large_data = "x".repeat(200);
        let input = json!({"large": large_data, "small": "ok"});

        let (result, ids) = blobify_inputs(input, 100, &store).await.unwrap();
        assert_eq!(ids.len(), 1);

        let obj = result.as_object().unwrap();
        assert!(is_blob_ref(&obj["large"]));
        assert_eq!(obj["small"], json!("ok"));
    }

    #[tokio::test]
    async fn test_blobify_zero_threshold() {
        let store = InMemoryStateStore::new();
        let input = json!({"data": "value"});
        let (result, ids) = blobify_inputs(input.clone(), 0, &store).await.unwrap();
        assert_eq!(result, input);
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn test_blobify_non_object() {
        let store = InMemoryStateStore::new();
        let input = json!("just a string");
        let (result, ids) = blobify_inputs(input.clone(), 1, &store).await.unwrap();
        assert_eq!(result, input);
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_blob_refs_simple() {
        let store = InMemoryStateStore::new();

        // Store a value
        let data = ValueRef::new(json!({"resolved": "data"}));
        let blob_id = store.put_blob(data.clone(), BlobType::Data).await.unwrap();

        // Create a value with a blob ref
        let blob_ref = BlobRef::new(blob_id, BlobType::Data, None);
        let input = json!({"field": blob_ref.to_value()});

        let result = resolve_blob_refs(input, &store).await.unwrap();
        assert_eq!(result, json!({"field": {"resolved": "data"}}));
    }

    #[tokio::test]
    async fn test_resolve_blob_refs_nested_in_array() {
        let store = InMemoryStateStore::new();

        let data = ValueRef::new(json!("hello"));
        let blob_id = store.put_blob(data, BlobType::Data).await.unwrap();

        let blob_ref = BlobRef::new(blob_id, BlobType::Data, None);
        let input = json!([blob_ref.to_value(), "plain"]);

        let result = resolve_blob_refs(input, &store).await.unwrap();
        assert_eq!(result, json!(["hello", "plain"]));
    }

    #[tokio::test]
    async fn test_resolve_no_blob_refs() {
        let store = InMemoryStateStore::new();
        let input = json!({"normal": "data", "nested": {"key": "value"}});
        let result = resolve_blob_refs(input.clone(), &store).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn test_blobify_then_resolve_round_trip() {
        let store = InMemoryStateStore::new();
        let large_data = "x".repeat(200);
        let input = json!({"large": large_data, "small": "ok"});

        let (blobified, _ids) = blobify_inputs(input.clone(), 100, &store).await.unwrap();
        assert_ne!(blobified, input); // Should have changed

        let resolved = resolve_blob_refs(blobified, &store).await.unwrap();
        assert_eq!(resolved, input); // Should round-trip back
    }
}
