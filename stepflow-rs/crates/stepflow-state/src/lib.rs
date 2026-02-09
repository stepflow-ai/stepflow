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

mod active_executions;
mod blob_store;
mod completion_notifier;
mod environment_ext;
mod error;
mod execution_journal;
mod in_memory;
pub mod journal_compliance;
mod lease_manager;
mod lease_types;
pub mod metadata_compliance;
mod metadata_store;
mod noop_journal;
mod noop_lease_manager;
mod state_store;

pub use active_executions::ActiveExecutions;
pub use blob_store::BlobStore;
pub use completion_notifier::RunCompletionNotifier;
pub use environment_ext::{
    ActiveExecutionsExt, BlobStoreExt, ExecutionJournalExt, LeaseManagerExt, MetadataStoreExt,
};
pub use error::{Result, StateError};
pub use execution_journal::{
    ExecutionJournal, ItemSteps, JournalEntry, JournalEvent, RootJournalInfo, SequenceNumber,
};
pub use in_memory::InMemoryStateStore;
pub use lease_manager::{LeaseError, LeaseManager, LeaseResult};
pub use lease_types::{LeaseInfo, OrchestratorId, OrchestratorInfo, OrphanedRun, RunRecoveryInfo};
pub use metadata_store::MetadataStore;
pub use noop_journal::NoOpJournal;
pub use noop_lease_manager::NoOpLeaseManager;
pub use state_store::CreateRunParams;
