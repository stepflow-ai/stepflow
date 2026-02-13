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

//! Filesystem-based blob store implementation.
//!
//! Stores blobs as individual JSON files on the local filesystem, using content-addressed
//! naming (SHA-256 hash). Each blob is stored with its type metadata in a wrapper format.
//!
//! Supports two modes:
//! - **Explicit directory**: Blobs are stored in a user-specified directory
//! - **Temporary directory**: A temp directory is created and cleaned up on drop

use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use stepflow_core::{BlobData, BlobId, BlobMetadata, BlobType, workflow::ValueRef};

use crate::StateError;

/// On-disk format for a stored blob.
#[derive(Serialize, Deserialize)]
struct StoredBlob {
    blob_type: BlobType,
    data: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

/// Directory handle that is either an owned path or a temp directory (auto-cleaned on drop).
enum DirHandle {
    /// User-specified directory path.
    Explicit(PathBuf),
    /// Temporary directory that is cleaned up when dropped.
    Temp(tempfile::TempDir),
}

impl DirHandle {
    fn path(&self) -> &Path {
        match self {
            DirHandle::Explicit(p) => p,
            DirHandle::Temp(t) => t.path(),
        }
    }
}

/// Filesystem-based implementation of [`BlobStore`](crate::BlobStore).
///
/// Each blob is stored as a JSON file named `<sha256_hash>.json` in the configured directory.
/// A two-character prefix subdirectory is used (like git) to avoid excessive files in a
/// single directory: `<dir>/ab/abcdef1234...json`.
pub struct FilesystemBlobStore {
    dir: DirHandle,
}

impl FilesystemBlobStore {
    /// Create a new filesystem blob store using an explicit directory.
    ///
    /// The directory will be created if it doesn't exist.
    pub async fn new(directory: PathBuf) -> error_stack::Result<Self, StateError> {
        std::fs::create_dir_all(&directory)
            .change_context(StateError::Initialization)
            .attach_printable_lazy(|| {
                format!(
                    "Failed to create blob store directory: {}",
                    directory.display()
                )
            })?;
        Ok(Self {
            dir: DirHandle::Explicit(directory),
        })
    }

    /// Create a new filesystem blob store using a temporary directory.
    ///
    /// The directory and all its contents are automatically deleted when the store is dropped.
    pub fn temp() -> error_stack::Result<Self, StateError> {
        let temp_dir = tempfile::tempdir()
            .change_context(StateError::Initialization)
            .attach_printable("Failed to create temporary blob store directory")?;
        Ok(Self {
            dir: DirHandle::Temp(temp_dir),
        })
    }

    /// Get the path where a blob with the given ID is stored.
    ///
    /// Uses a two-character prefix directory structure: `<dir>/ab/abcdef1234...json`
    fn blob_path(&self, blob_id: &BlobId) -> PathBuf {
        let hash = blob_id.as_str();
        let prefix = &hash[..2];
        self.dir.path().join(prefix).join(format!("{hash}.json"))
    }
}

impl crate::BlobStore for FilesystemBlobStore {
    fn put_blob(
        &self,
        data: ValueRef,
        blob_type: BlobType,
        metadata: BlobMetadata,
    ) -> BoxFuture<'_, error_stack::Result<BlobId, StateError>> {
        async move {
            let blob_id =
                BlobId::compute(&data, &blob_type).change_context(StateError::Serialization)?;

            let path = self.blob_path(&blob_id);

            // Skip if already exists (content-addressed, so identical content)
            if path.exists() {
                return Ok(blob_id);
            }

            // Ensure the prefix directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .change_context(StateError::Internal)
                    .attach_printable_lazy(|| {
                        format!("Failed to create prefix directory: {}", parent.display())
                    })?;
            }

            let stored = StoredBlob {
                blob_type,
                data: data.as_ref().clone(),
                filename: metadata.filename,
            };

            let json =
                serde_json::to_vec_pretty(&stored).change_context(StateError::Serialization)?;

            // Write atomically: write to temp file then rename to avoid partial reads
            let temp_path = path.with_extension("json.tmp");
            std::fs::write(&temp_path, &json)
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("Failed to write blob file: {}", temp_path.display())
                })?;

            std::fs::rename(&temp_path, &path)
                .change_context(StateError::Internal)
                .attach_printable_lazy(|| {
                    format!("Failed to rename blob file: {}", path.display())
                })?;

            Ok(blob_id)
        }
        .boxed()
    }

    fn get_blob_opt(
        &self,
        blob_id: &BlobId,
    ) -> BoxFuture<'_, error_stack::Result<Option<BlobData>, StateError>> {
        let blob_id = blob_id.clone();
        async move {
            let path = self.blob_path(&blob_id);

            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => {
                    return Err(error_stack::report!(StateError::Internal))
                        .attach_printable(format!("Failed to read blob file: {}", path.display()))
                        .attach_printable(e);
                }
            };

            let stored: StoredBlob = serde_json::from_slice(&bytes)
                .change_context(StateError::Serialization)
                .attach_printable_lazy(|| {
                    format!("Failed to deserialize blob file: {}", path.display())
                })?;

            let metadata = BlobMetadata {
                filename: stored.filename,
            };
            let blob_data = BlobData::with_metadata(
                stepflow_core::blob::BlobValue::from_value_ref(
                    ValueRef::new(stored.data),
                    stored.blob_type,
                )
                .change_context(StateError::Serialization)?,
                blob_id,
                metadata,
            );

            Ok(Some(blob_data))
        }
        .boxed()
    }
}

/// Configuration for the filesystem blob store.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemBlobStoreConfig {
    /// Directory path for storing blobs. If not specified, a temporary directory is used.
    #[serde(default)]
    pub directory: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlobStore as _;
    use crate::blob_compliance::BlobStoreComplianceTests;

    #[tokio::test]
    async fn filesystem_blob_compliance_temp() {
        BlobStoreComplianceTests::run_all_isolated(|| async {
            FilesystemBlobStore::temp().unwrap()
        })
        .await;
    }

    #[tokio::test]
    async fn filesystem_blob_compliance_explicit_dir() {
        let temp = tempfile::tempdir().unwrap();
        let store = FilesystemBlobStore::new(temp.path().to_path_buf())
            .await
            .unwrap();
        BlobStoreComplianceTests::run_all(&store).await;
    }

    #[tokio::test]
    async fn temp_dir_cleanup() {
        let dir_path;
        {
            let store = FilesystemBlobStore::temp().unwrap();
            dir_path = store.dir.path().to_path_buf();

            // Store a blob
            let data = ValueRef::new(serde_json::json!({"cleanup": "test"}));
            store
                .put_blob(data, BlobType::Data, Default::default())
                .await
                .unwrap();

            assert!(
                dir_path.exists(),
                "Directory should exist while store is alive"
            );
        }
        // Store dropped - temp directory should be cleaned up
        assert!(
            !dir_path.exists(),
            "Temp directory should be cleaned up after store is dropped"
        );
    }
}
