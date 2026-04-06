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

//! Blob reference (`$blob` sentinel) for auto-blobification of large values.
//!
//! A blob ref is a JSON object with a `$blob` key indicating the value is stored
//! in the blob store. This module provides:
//!
//! - **Input de-blobification**: `resolve_blob_refs` recursively walks a JSON tree,
//!   finds blob refs, fetches their data in parallel, and replaces them inline.
//! - **Output blobification**: `blobify_output` checks each top-level field of an
//!   output object and stores large fields as blobs, replacing them with blob refs.
//!
//! # Blob ref format
//!
//! ```json
//! {"$blob": "<64-char-sha256-hex>", "blobType": "data", "size": 12345}
//! ```

use serde_json::{Map, Value};
use tracing::debug;

use crate::ComponentContext;
use crate::error::ContextError;

/// The JSON key used to identify blob references.
const BLOB_REF_KEY: &str = "$blob";

/// Maximum recursion depth when resolving blob refs (prevents infinite loops
/// if resolved data itself contains blob refs).
const MAX_RESOLVE_DEPTH: usize = 8;

/// Check whether a JSON value is a blob ref (object with `$blob` key containing
/// a 64-character hex string).
fn is_blob_ref(value: &Value) -> Option<&str> {
    let obj = value.as_object()?;
    let blob_id = obj.get(BLOB_REF_KEY)?.as_str()?;
    if blob_id.len() == 64 {
        Some(blob_id)
    } else {
        None
    }
}

/// Collect all blob ref IDs from a JSON value tree.
fn collect_blob_ids(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::Object(obj) => {
            if let Some(blob_id) = is_blob_ref(&Value::Object(obj.clone())) {
                ids.push(blob_id.to_string());
            } else {
                for v in obj.values() {
                    collect_blob_ids(v, ids);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_blob_ids(item, ids);
            }
        }
        _ => {}
    }
}

/// Replace blob refs in a JSON value with their pre-fetched data (no I/O).
fn replace_blob_refs(value: Value, resolved: &std::collections::HashMap<String, Value>) -> Value {
    match value {
        Value::Object(obj) => {
            // Check if this object itself is a blob ref
            if let Some(blob_id) = obj.get(BLOB_REF_KEY).and_then(|v| v.as_str())
                && blob_id.len() == 64
                && let Some(data) = resolved.get(blob_id)
            {
                return data.clone();
            }
            // Otherwise recurse into fields
            let new_obj: Map<String, Value> = obj
                .into_iter()
                .map(|(k, v)| (k, replace_blob_refs(v, resolved)))
                .collect();
            Value::Object(new_obj)
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| replace_blob_refs(v, resolved))
                .collect(),
        ),
        other => other,
    }
}

/// Resolve all blob refs in a JSON value by fetching their data from the blob store.
///
/// Blob refs at all levels are collected and fetched in parallel, then substituted.
/// If resolved data itself contains blob refs, another round of fetching is performed
/// (up to [`MAX_RESOLVE_DEPTH`] rounds).
pub(crate) async fn resolve_blob_refs(
    value: Value,
    ctx: &ComponentContext,
) -> Result<Value, ContextError> {
    resolve_blob_refs_inner(value, ctx, 0).await
}

fn resolve_blob_refs_inner<'a>(
    value: Value,
    ctx: &'a ComponentContext,
    depth: usize,
) -> futures::future::BoxFuture<'a, Result<Value, ContextError>> {
    Box::pin(async move {
        if depth >= MAX_RESOLVE_DEPTH {
            return Ok(value);
        }

        // Collect all blob IDs in the current tree
        let mut ids = Vec::new();
        collect_blob_ids(&value, &mut ids);
        ids.sort_unstable();
        ids.dedup();

        if ids.is_empty() {
            return Ok(value);
        }

        debug!(count = ids.len(), depth, "Resolving blob refs in parallel");

        // Fetch all blobs in parallel
        let fetches: Vec<_> = ids
            .iter()
            .map(|id| {
                let ctx = ctx.clone();
                let id = id.clone();
                async move {
                    let data = ctx.get_blob(&id).await?;
                    Ok::<_, ContextError>((id, data))
                }
            })
            .collect();

        let results = futures::future::try_join_all(fetches).await?;
        let resolved: std::collections::HashMap<String, Value> = results.into_iter().collect();

        // Replace refs with fetched data
        let result = replace_blob_refs(value, &resolved);

        // Check if resolved data contains more blob refs
        let mut next_ids = Vec::new();
        collect_blob_ids(&result, &mut next_ids);
        if !next_ids.is_empty() {
            return resolve_blob_refs_inner(result, ctx, depth + 1).await;
        }

        Ok(result)
    })
}

/// Replace large top-level fields in an output object with blob refs.
///
/// Each top-level field is individually checked against `threshold`. Fields whose
/// JSON serialization exceeds the threshold are stored as blobs and replaced with
/// blob refs. Non-object values pass through unchanged.
///
/// Returns the (possibly modified) output value.
pub(crate) async fn blobify_output(
    output: Value,
    threshold: usize,
    ctx: &ComponentContext,
) -> Result<Value, ContextError> {
    if threshold == 0 {
        return Ok(output);
    }

    let obj = match output {
        Value::Object(obj) => obj,
        other => return Ok(other),
    };

    let mut result = Map::with_capacity(obj.len());

    for (key, value) in obj {
        // Skip values that are already blob refs
        if is_blob_ref(&value).is_some() {
            result.insert(key, value);
            continue;
        }

        // Check serialized size of this field
        let serialized = serde_json::to_string(&value)?;
        let size = serialized.len();

        if size > threshold {
            let blob_id = ctx.put_blob(value).await?;
            debug!(field = %key, size, blob_id = %&blob_id[..12], "Blobified output field");

            let mut ref_obj = Map::new();
            ref_obj.insert(BLOB_REF_KEY.to_string(), Value::String(blob_id));
            ref_obj.insert("blobType".to_string(), Value::String("data".to_string()));
            ref_obj.insert("size".to_string(), Value::Number(size.into()));
            result.insert(key, Value::Object(ref_obj));
        } else {
            result.insert(key, value);
        }
    }

    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_is_blob_ref_valid() {
        let hex = "a".repeat(64);
        let value = json!({"$blob": hex});
        assert_eq!(is_blob_ref(&value), Some(hex.as_str()));
    }

    #[test]
    fn test_is_blob_ref_wrong_length() {
        let value = json!({"$blob": "tooshort"});
        assert_eq!(is_blob_ref(&value), None);
    }

    #[test]
    fn test_is_blob_ref_missing_key() {
        let value = json!({"other": "value"});
        assert_eq!(is_blob_ref(&value), None);
    }

    #[test]
    fn test_is_blob_ref_not_object() {
        assert_eq!(is_blob_ref(&json!("string")), None);
        assert_eq!(is_blob_ref(&json!(42)), None);
        assert_eq!(is_blob_ref(&json!(null)), None);
    }

    #[test]
    fn test_collect_blob_ids_nested() {
        let hex1 = "a".repeat(64);
        let hex2 = "b".repeat(64);
        let value = json!({
            "field1": {"$blob": hex1},
            "field2": {
                "nested": {"$blob": hex2}
            },
            "field3": "plain"
        });
        let mut ids = Vec::new();
        collect_blob_ids(&value, &mut ids);
        ids.sort();
        assert_eq!(ids, vec![hex1, hex2]);
    }

    #[test]
    fn test_collect_blob_ids_in_array() {
        let hex = "c".repeat(64);
        let value = json!([{"$blob": hex}, "plain", 42]);
        let mut ids = Vec::new();
        collect_blob_ids(&value, &mut ids);
        assert_eq!(ids, vec![hex]);
    }

    #[test]
    fn test_collect_blob_ids_empty() {
        let value = json!({"field": "value", "nested": {"a": 1}});
        let mut ids = Vec::new();
        collect_blob_ids(&value, &mut ids);
        assert!(ids.is_empty());
    }

    #[test]
    fn test_replace_blob_refs() {
        let hex = "d".repeat(64);
        let value = json!({
            "field1": {"$blob": hex},
            "field2": "unchanged"
        });
        let mut resolved = std::collections::HashMap::new();
        resolved.insert(hex, json!({"resolved": "data"}));

        let result = replace_blob_refs(value, &resolved);
        assert_eq!(
            result,
            json!({
                "field1": {"resolved": "data"},
                "field2": "unchanged"
            })
        );
    }

    #[test]
    fn test_replace_blob_refs_unresolved() {
        let hex = "e".repeat(64);
        let value = json!({"$blob": hex});
        let resolved = std::collections::HashMap::new();
        // Unresolved blob refs pass through unchanged
        let result = replace_blob_refs(value.clone(), &resolved);
        assert_eq!(result, value);
    }

    #[test]
    fn test_replace_blob_refs_nested_in_array() {
        let hex = "f".repeat(64);
        let value = json!([{"$blob": hex}, "keep"]);
        let mut resolved = std::collections::HashMap::new();
        resolved.insert(hex, json!("fetched"));

        let result = replace_blob_refs(value, &resolved);
        assert_eq!(result, json!(["fetched", "keep"]));
    }
}
