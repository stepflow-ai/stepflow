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

//! Trait and types for storing execution checkpoints.
//!
//! Checkpoints are keyed by `root_run_id` and only the latest checkpoint
//! for a given root is needed. Implementations may discard older ones.
//!
//! This is a separate trait from `BlobStore` because checkpoints are ephemeral
//! (only the latest matters, old ones should be replaced/deleted), which is a
//! lifecycle mismatch with the blob store (immutable, content-addressed, no delete).

use bytes::Bytes;
use futures::future::{BoxFuture, FutureExt as _};
use uuid::Uuid;

use crate::{SequenceNumber, StateError};

/// Stored checkpoint data returned by the store.
#[derive(Debug, Clone)]
pub struct StoredCheckpoint {
    /// Journal sequence number this checkpoint reflects.
    pub sequence: SequenceNumber,
    /// Serialized checkpoint data (MessagePack encoded).
    pub data: Bytes,
}

/// Trait for storing and retrieving execution checkpoints.
///
/// Checkpoints are keyed by `root_run_id`. Only the latest checkpoint
/// for a given root is needed; implementations may discard older ones.
pub trait CheckpointStore: Send + Sync {
    /// Initialize the checkpoint store backend (e.g., create tables).
    ///
    /// Called by the configuration layer after the store is created and before
    /// it is used. Default is a no-op (matches the pattern used by
    /// MetadataStore, BlobStore, and ExecutionJournal).
    fn initialize_checkpoint_store(&self) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async { Ok(()) }.boxed()
    }

    /// Store a checkpoint, replacing any previous checkpoint for this root_run_id.
    fn put_checkpoint(
        &self,
        root_run_id: Uuid,
        sequence: SequenceNumber,
        data: Bytes,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;

    /// Get the latest checkpoint for a root_run_id.
    /// Returns None if no checkpoint exists.
    fn get_latest_checkpoint(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<StoredCheckpoint>, StateError>>;

    /// Delete all checkpoints for a root_run_id.
    ///
    /// Called after a run completes to free storage. Implementations
    /// should treat this as best-effort — a failure here should not prevent
    /// run completion.
    fn delete_checkpoints(
        &self,
        root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>>;
}

/// No-op checkpoint store that discards all checkpoints.
///
/// Used as the default when checkpointing is not configured,
/// keeping the feature opt-in without requiring code changes.
pub struct NoOpCheckpointStore;

impl CheckpointStore for NoOpCheckpointStore {
    fn put_checkpoint(
        &self,
        _root_run_id: Uuid,
        _sequence: SequenceNumber,
        _data: Bytes,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async { Ok(()) }.boxed()
    }

    fn get_latest_checkpoint(
        &self,
        _root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<Option<StoredCheckpoint>, StateError>> {
        async { Ok(None) }.boxed()
    }

    fn delete_checkpoints(
        &self,
        _root_run_id: Uuid,
    ) -> BoxFuture<'_, error_stack::Result<(), StateError>> {
        async { Ok(()) }.boxed()
    }
}
