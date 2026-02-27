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

//! Shared types for the recovery module.

use std::collections::HashMap;

use crate::RunState;

/// State recovered from either a checkpoint or full journal replay.
///
/// Both recovery paths (checkpoint-accelerated and full journal replay)
/// produce the same output: the root run's state, a subflow deduplication
/// map, and any recovered subflow states.
pub(super) struct RecoveredState {
    /// The root run's execution state.
    pub run_state: RunState,
    /// Subflow deduplication map.
    ///
    /// Key: `(parent_run_id, item_index, step_index, subflow_key)`.
    /// Value: `subflow_run_id`.
    pub subflow_map: HashMap<(uuid::Uuid, u32, usize, uuid::Uuid), uuid::Uuid>,
    /// Additional (subflow) RunStates keyed by run_id.
    pub subflow_runs: HashMap<uuid::Uuid, RunState>,
    /// Subflow run IDs that were in-flight at crash time (initialized but not completed).
    ///
    /// These already have a `RunInitialized` journal event from the original execution,
    /// so recovery must skip writing a duplicate.
    pub inflight_subflow_run_ids: std::collections::HashSet<uuid::Uuid>,
}
