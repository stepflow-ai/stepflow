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
//! Stores blobs as individual files on the local filesystem, using content-addressed
//! naming (SHA-256 hash) with a trailing-metadata binary format.
//!
//! ## File Format
//!
//! Each blob file uses a trailing-metadata layout:
//!
//! ```text
//! [CONTENT BYTES (variable)]  [METADATA JSON (variable)]  [CONTENT_LENGTH (4 bytes, LE u32)]
//! ```
//!
//! Reading strategy:
//! 1. Read last 4 bytes as `u32` (little-endian) → `content_length`
//! 2. Content is bytes `0..content_length`
//! 3. Metadata JSON is bytes `content_length..(file_size - 4)`
//!
//! ## Storage Modes
//!
//! - **Explicit directory**: Blobs are stored in a user-specified directory
//! - **Temporary directory**: A temp directory is created and cleaned up on drop

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use error_stack::ResultExt as _;
use futures::future::{BoxFuture, FutureExt as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use stepflow_core::{BlobData, BlobId, BlobMetadata, BlobType, workflow::ValueRef};

use crate::StateError;

/// On-disk metadata stored after blob content.
#[derive(Serialize, Deserialize)]
struct BlobFileMetadata {
    blob_type: BlobType,
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
/// Each blob is stored as a file named `<sha256_hash>` (no extension) in the configured
/// directory. A two-character prefix subdirectory is used (like git) to avoid excessive
/// files in a single directory: `<dir>/ab/abcdef1234...`.
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

    /// Get the path where a blob with the given ID is stored (no extension).
    ///
    /// Uses a two-character prefix directory structure: `<dir>/ab/abcdef1234...`
    fn blob_path(&self, blob_id: &BlobId) -> PathBuf {
        let hash = blob_id.as_str();
        let prefix = &hash[..2];
        self.dir.path().join(prefix).join(hash)
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

            // Get raw content bytes based on blob type
            let content_bytes = match blob_type {
                BlobType::Binary => {
                    // Decode base64 from the ValueRef to get raw bytes
                    let base64_str = data
                        .as_ref()
                        .as_str()
                        .ok_or_else(|| error_stack::report!(StateError::Serialization))
                        .attach_printable("Binary blob data is not a string")?;
                    BASE64_STANDARD
                        .decode(base64_str)
                        .change_context(StateError::Serialization)
                        .attach_printable("Failed to decode base64 binary blob data")?
                }
                BlobType::Data | BlobType::Flow => {
                    // Serialize JSON compactly
                    serde_json::to_vec(data.as_ref())
                        .change_context(StateError::Serialization)
                        .attach_printable("Failed to serialize blob data as JSON")?
                }
            };

            let file_metadata = BlobFileMetadata {
                blob_type,
                filename: metadata.filename,
            };
            let metadata_bytes = serde_json::to_vec(&file_metadata)
                .change_context(StateError::Serialization)
                .attach_printable("Failed to serialize blob file metadata")?;

            let content_length = content_bytes.len() as u32;

            // Build complete buffer: [content][metadata_json][content_length LE u32]
            let mut buf =
                Vec::with_capacity(content_bytes.len() + metadata_bytes.len() + 4);
            buf.extend_from_slice(&content_bytes);
            buf.extend_from_slice(&metadata_bytes);
            buf.extend_from_slice(&content_length.to_le_bytes());

            // Write atomically: write to .tmp then rename
            let temp_path = path.with_extension("tmp");
            std::fs::write(&temp_path, &buf)
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

            let file_size = bytes.len();

            if file_size < 4 {
                return Err(error_stack::report!(StateError::Serialization))
                    .attach_printable(format!(
                        "Corrupted blob file (too short: {} bytes): {}",
                        file_size,
                        path.display()
                    ));
            }

            // Read last 4 bytes as content_length (u32 LE)
            let content_length = u32::from_le_bytes(
                bytes[file_size - 4..].try_into().unwrap(),
            ) as usize;

            if content_length > file_size - 4 {
                return Err(error_stack::report!(StateError::Serialization))
                    .attach_printable(format!(
                        "Corrupted blob file (content_length {} exceeds available {} bytes): {}",
                        content_length,
                        file_size - 4,
                        path.display()
                    ));
            }

            let content = &bytes[..content_length];
            let metadata_json = &bytes[content_length..file_size - 4];

            let file_metadata: BlobFileMetadata = serde_json::from_slice(metadata_json)
                .change_context(StateError::Serialization)
                .attach_printable_lazy(|| {
                    format!(
                        "Failed to deserialize blob file metadata: {}",
                        path.display()
                    )
                })?;

            let blob_value = match file_metadata.blob_type {
                BlobType::Binary => {
                    stepflow_core::blob::BlobValue::Binary(content.to_vec())
                }
                BlobType::Data | BlobType::Flow => {
                    let json_value: serde_json::Value = serde_json::from_slice(content)
                        .change_context(StateError::Serialization)
                        .attach_printable_lazy(|| {
                            format!(
                                "Failed to deserialize blob content as JSON: {}",
                                path.display()
                            )
                        })?;
                    stepflow_core::blob::BlobValue::from_value_ref(
                        ValueRef::new(json_value),
                        file_metadata.blob_type,
                    )
                    .change_context(StateError::Serialization)?
                }
            };

            let metadata = BlobMetadata {
                filename: file_metadata.filename,
            };

            Ok(Some(BlobData::with_metadata(blob_value, blob_id, metadata)))
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

    // =========================================================================
    // Corruption and Edge-Case Tests
    // =========================================================================

    /// Helper: compute the expected file path for a blob ID within a store.
    fn blob_path_for(store: &FilesystemBlobStore, blob_id: &BlobId) -> PathBuf {
        store.blob_path(blob_id)
    }

    /// Helper: create a fake blob ID and ensure its parent directory exists in the store.
    fn setup_corrupt_file(
        store: &FilesystemBlobStore,
        content: &[u8],
    ) -> BlobId {
        let blob_id = BlobId::from_content(&ValueRef::new(serde_json::json!({"corrupt": "test"})))
            .unwrap();
        let path = blob_path_for(store, &blob_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        blob_id
    }

    #[tokio::test]
    async fn test_corrupted_file_too_short() {
        let store = FilesystemBlobStore::temp().unwrap();
        // Write only 3 bytes — less than the 4-byte footer
        let blob_id = setup_corrupt_file(&store, &[0x01, 0x02, 0x03]);

        let result = store.get_blob_opt(&blob_id).await;
        assert!(result.is_err(), "Should error on file shorter than 4 bytes");
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("too short"),
            "Error should mention file is too short: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_corrupted_file_no_footer() {
        let store = FilesystemBlobStore::temp().unwrap();
        // Write 100 bytes of 0xAB — last 4 bytes interpreted as content_length will be garbage
        let blob_id = setup_corrupt_file(&store, &[0xAB; 100]);

        let result = store.get_blob_opt(&blob_id).await;
        assert!(
            result.is_err(),
            "Should error when last 4 bytes produce nonsensical content_length"
        );
    }

    #[tokio::test]
    async fn test_corrupted_content_length_too_large() {
        let store = FilesystemBlobStore::temp().unwrap();
        // Build a file where content_length (last 4 bytes) exceeds available space.
        // 10 bytes of padding + u32 LE of 9999
        let mut data = vec![0u8; 10];
        data.extend_from_slice(&9999u32.to_le_bytes());

        let blob_id = setup_corrupt_file(&store, &data);

        let result = store.get_blob_opt(&blob_id).await;
        assert!(result.is_err(), "Should error when content_length is too large");
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("content_length") || err_msg.contains("exceeds"),
            "Error should mention invalid content length: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_corrupted_metadata_invalid_json() {
        let store = FilesystemBlobStore::temp().unwrap();
        // Build a file with: 5 bytes of "content", 5 bytes of garbage "metadata", footer = 5
        let mut data = Vec::new();
        data.extend_from_slice(b"hello"); // content (5 bytes)
        data.extend_from_slice(b"!@#$%"); // invalid JSON metadata (5 bytes)
        data.extend_from_slice(&5u32.to_le_bytes()); // content_length = 5

        let blob_id = setup_corrupt_file(&store, &data);

        let result = store.get_blob_opt(&blob_id).await;
        assert!(
            result.is_err(),
            "Should error when metadata is not valid JSON"
        );
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("metadata"),
            "Error should mention metadata deserialization: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_corrupted_content_length_zero() {
        // content_length=0 means no content bytes, everything before the footer is metadata.
        // This is technically valid if the metadata is valid JSON and represents an empty blob.
        let store = FilesystemBlobStore::temp().unwrap();

        let metadata_json = serde_json::to_vec(&BlobFileMetadata {
            blob_type: BlobType::Data,
            filename: None,
        })
        .unwrap();

        let mut data = Vec::new();
        // No content bytes (content_length = 0)
        data.extend_from_slice(&metadata_json);
        data.extend_from_slice(&0u32.to_le_bytes());

        let blob_id = setup_corrupt_file(&store, &data);

        // content_length=0 means content slice is empty. Parsing empty slice as JSON will fail
        // because an empty byte slice is not valid JSON. This is the expected behavior — a blob
        // with zero content length for a Data type is effectively corrupted.
        let result = store.get_blob_opt(&blob_id).await;
        assert!(
            result.is_err(),
            "Zero content_length for Data blob should error (empty JSON is invalid)"
        );
    }

    #[tokio::test]
    async fn test_empty_file() {
        let store = FilesystemBlobStore::temp().unwrap();
        let blob_id = setup_corrupt_file(&store, &[]);

        let result = store.get_blob_opt(&blob_id).await;
        assert!(result.is_err(), "Should error on empty file");
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("too short"),
            "Error should mention file is too short: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_binary_blob_direct_bytes() {
        let store = FilesystemBlobStore::temp().unwrap();
        let raw_bytes = b"Hello, binary world! \x00\x01\x02\xff";

        let blob_id = store
            .put_blob_binary(raw_bytes, Default::default())
            .await
            .unwrap();

        // Read the raw file from disk
        let path = blob_path_for(&store, &blob_id);
        let file_bytes = std::fs::read(&path).unwrap();
        let file_size = file_bytes.len();

        // Extract content_length from footer
        let content_length = u32::from_le_bytes(
            file_bytes[file_size - 4..].try_into().unwrap(),
        ) as usize;

        // Verify the content bytes are the original raw bytes (not base64-encoded)
        assert_eq!(
            &file_bytes[..content_length],
            raw_bytes,
            "On-disk content should be raw bytes, not base64"
        );
    }

    #[tokio::test]
    async fn test_json_blob_direct_content() {
        let store = FilesystemBlobStore::temp().unwrap();
        let original = serde_json::json!({"key": "value", "number": 42});
        let data = ValueRef::new(original.clone());

        let blob_id = store
            .put_blob(data, BlobType::Data, Default::default())
            .await
            .unwrap();

        // Read the raw file from disk
        let path = blob_path_for(&store, &blob_id);
        let file_bytes = std::fs::read(&path).unwrap();
        let file_size = file_bytes.len();

        // Extract content_length from footer
        let content_length = u32::from_le_bytes(
            file_bytes[file_size - 4..].try_into().unwrap(),
        ) as usize;

        // Verify content bytes are valid JSON matching the original value
        let stored_json: serde_json::Value =
            serde_json::from_slice(&file_bytes[..content_length]).unwrap();
        assert_eq!(
            stored_json, original,
            "On-disk content should be raw JSON, not wrapped"
        );
    }

    #[tokio::test]
    async fn test_no_tmp_files_after_successful_write() {
        let store = FilesystemBlobStore::temp().unwrap();
        let data = ValueRef::new(serde_json::json!({"tmp_test": true}));

        let blob_id = store
            .put_blob(data, BlobType::Data, Default::default())
            .await
            .unwrap();

        // List all files in the blob's prefix directory
        let path = blob_path_for(&store, &blob_id);
        let prefix_dir = path.parent().unwrap();
        let entries: Vec<_> = std::fs::read_dir(prefix_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        for entry in &entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            assert!(
                !name_str.ends_with(".tmp"),
                "No .tmp files should remain after successful write, found: {name_str}"
            );
        }
    }

    #[tokio::test]
    async fn test_tmp_file_ignored_on_read() {
        let store = FilesystemBlobStore::temp().unwrap();
        let data = ValueRef::new(serde_json::json!({"tmp_ignore": true}));

        // Write a real blob
        let blob_id = store
            .put_blob(data.clone(), BlobType::Data, Default::default())
            .await
            .unwrap();

        // Manually create a .tmp file alongside it
        let path = blob_path_for(&store, &blob_id);
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, b"garbage tmp data").unwrap();

        // get_blob_opt should still read the correct blob
        let blob_data = store.get_blob_opt(&blob_id).await.unwrap().unwrap();
        assert_eq!(blob_data.blob_type(), BlobType::Data);
        assert_eq!(
            blob_data.data().as_ref(),
            &serde_json::json!({"tmp_ignore": true})
        );

        // put_blob with the same content should still return the existing blob ID (dedup)
        let blob_id2 = store
            .put_blob(data, BlobType::Data, Default::default())
            .await
            .unwrap();
        assert_eq!(blob_id, blob_id2, "Dedup should find the existing blob");
    }
}
