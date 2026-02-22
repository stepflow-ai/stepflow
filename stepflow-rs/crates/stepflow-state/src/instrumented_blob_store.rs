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

//! Instrumented blob store wrapper that records OTel metrics for blob operations.

use std::sync::Arc;
use std::time::Instant;

use futures::future::{BoxFuture, FutureExt as _};
use stepflow_core::{BlobId, BlobMetadata, blob::BlobType};
use stepflow_observability::{record_blob_get, record_blob_put};

use crate::StateError;
use crate::blob_store::{BlobStore, RawBlob};

/// A [`BlobStore`] wrapper that records OTel metrics for every put/get operation.
///
/// Delegates to an inner `BlobStore` and transparently records:
/// - Bytes transferred (put and get counters)
/// - Operation duration (put and get histograms)
/// - Blob size distribution (histogram)
///
/// All metrics are tagged with `blob_type`.
pub struct InstrumentedBlobStore {
    inner: Arc<dyn BlobStore>,
}

impl InstrumentedBlobStore {
    /// Wrap a blob store with metrics instrumentation.
    pub fn new(inner: Arc<dyn BlobStore>) -> Self {
        Self { inner }
    }
}

impl BlobStore for InstrumentedBlobStore {
    fn put_blob(
        &self,
        content: &[u8],
        blob_type: BlobType,
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        let size = content.len() as u64;
        let blob_type_label = blob_type.to_string();
        let fut = self.inner.put_blob(content, blob_type, metadata);
        async move {
            let start = Instant::now();
            let result = fut.await;
            let elapsed = start.elapsed().as_secs_f64();
            if result.is_ok() {
                record_blob_put(&blob_type_label, size, elapsed);
            }
            result
        }
        .boxed()
    }

    fn get_blob(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<RawBlob>, StateError>> {
        let fut = self.inner.get_blob(blob_id);
        async move {
            let start = Instant::now();
            let result = fut.await;
            let elapsed = start.elapsed().as_secs_f64();
            if let Ok(Some(ref raw)) = result {
                record_blob_get(
                    &raw.blob_type.to_string(),
                    raw.content.len() as u64,
                    elapsed,
                );
            }
            result
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStateStore;
    use crate::blob_compliance::BlobStoreComplianceTests;
    use serde_json::json;

    fn make_instrumented() -> InstrumentedBlobStore {
        InstrumentedBlobStore::new(Arc::new(InMemoryStateStore::new()))
    }

    #[tokio::test]
    async fn test_instrumented_put_get_round_trip() {
        let store = make_instrumented();
        let data = json!({"hello": "world"});
        let content = serde_json::to_vec(&data).unwrap();

        let blob_id = store
            .put_blob(&content, BlobType::Data, BlobMetadata::default())
            .await
            .expect("put should succeed");

        let raw = store
            .get_blob(&blob_id)
            .await
            .expect("get should succeed")
            .expect("blob should exist");

        let retrieved: serde_json::Value = serde_json::from_slice(&raw.content).unwrap();
        assert_eq!(retrieved, data);
        assert_eq!(raw.blob_type, BlobType::Data);
    }

    #[tokio::test]
    async fn test_instrumented_get_not_found() {
        let store = make_instrumented();
        let fake_content = serde_json::to_vec(&json!({"nope": true})).unwrap();
        let fake_id = BlobId::from_binary(&fake_content).unwrap();

        let result = store
            .get_blob(&fake_id)
            .await
            .expect("get should not error");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_instrumented_blob_compliance() {
        BlobStoreComplianceTests::run_all_isolated(|| async { make_instrumented() }).await;
    }
}
