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

//! Execution checkpoint types for periodic state snapshots.
//!
//! Checkpoints serialize the in-memory execution state at a point in time
//! so that recovery only needs to replay journal events *after* the checkpoint,
//! rather than replaying from sequence 0.
//!
//! Checkpoint data is serialized using MessagePack (via `rmp-serde`) for
//! compact binary representation.

use std::collections::HashMap;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use stepflow_core::values::ValueRef;
use stepflow_core::{BlobId, FlowResult};
use stepflow_state::SequenceNumber;
use uuid::Uuid;

use crate::RunState;

/// Current checkpoint format version.
const CHECKPOINT_VERSION: u32 = 1;

/// Serializable checkpoint of execution state at a point in time.
///
/// Contains everything needed to reconstruct the in-memory execution state
/// without replaying the journal from the beginning. The `sequence` field
/// records which journal position this checkpoint reflects, so recovery
/// only needs to replay events after that point.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointData {
    /// Format version for forward compatibility.
    pub version: u32,
    /// Journal sequence number this checkpoint reflects.
    pub sequence: SequenceNumber,
    /// State for each run in the execution tree.
    pub runs: Vec<RunCheckpoint>,
    /// Subflow deduplication map entries.
    pub subflow_map: Vec<SubflowMapping>,
}

/// Checkpoint of a single run's state.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunCheckpoint {
    /// Unique run ID.
    pub run_id: Uuid,
    /// Flow ID (content hash).
    pub flow_id: BlobId,
    /// Root run ID (same as run_id for top-level runs).
    pub root_run_id: Uuid,
    /// Parent run ID (None for top-level runs).
    pub parent_run_id: Option<Uuid>,
    /// Input values for each item in the batch.
    pub inputs: Vec<ValueRef>,
    /// Variable values shared across items.
    pub variables: HashMap<String, ValueRef>,
    /// Per-item execution state.
    pub items: Vec<ItemCheckpoint>,
}

/// Checkpoint of a single item's execution state.
#[derive(Debug, Serialize, Deserialize)]
pub struct ItemCheckpoint {
    /// Indices of completed steps.
    pub completed: Vec<usize>,
    /// (step_index, result) for completed steps.
    pub results: Vec<(usize, FlowResult)>,
    /// Indices of needed steps.
    pub needed: Vec<usize>,
    /// (step_index, [dependency_indices]) for steps with non-empty waiting_on.
    pub waiting_on: Vec<(usize, Vec<usize>)>,
    /// Per-step attempt counts (full Vec, indexed by step).
    pub attempts: Vec<u32>,
}

/// A single entry from the subflow deduplication map.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubflowMapping {
    /// Parent run that submitted the subflow.
    pub parent_run_id: Uuid,
    /// Item index within the parent run.
    pub item_index: u32,
    /// Step index within the parent run.
    pub step_index: usize,
    /// Caller-provided deduplication key.
    pub subflow_key: Uuid,
    /// Run ID assigned to the subflow.
    pub subflow_run_id: Uuid,
}

/// Error type for checkpoint deserialization.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointDeserializeError {
    /// MessagePack decoding failed.
    #[error("failed to decode checkpoint: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    /// Checkpoint version doesn't match the current version.
    #[error("checkpoint version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },
}

impl CheckpointData {
    /// Capture a checkpoint from the current execution state.
    ///
    /// Synchronously snapshots all `RunState` objects and the subflow
    /// deduplication map at the given journal sequence position.
    pub fn capture(
        runs: &HashMap<Uuid, RunState>,
        subflow_map: &HashMap<(Uuid, u32, usize, Uuid), Uuid>,
        sequence: SequenceNumber,
    ) -> Self {
        // Sort runs by run_id for deterministic checkpoint output.
        let mut sorted_ids: Vec<_> = runs.keys().collect();
        sorted_ids.sort();
        let run_checkpoints = sorted_ids
            .into_iter()
            .map(|id| runs[id].to_checkpoint())
            .collect();
        let subflow_mappings = subflow_map
            .iter()
            .map(
                |(&(parent_run_id, item_index, step_index, subflow_key), &subflow_run_id)| {
                    SubflowMapping {
                        parent_run_id,
                        item_index,
                        step_index,
                        subflow_key,
                        subflow_run_id,
                    }
                },
            )
            .collect();

        Self {
            version: CHECKPOINT_VERSION,
            sequence,
            runs: run_checkpoints,
            subflow_map: subflow_mappings,
        }
    }

    /// Serialize to MessagePack bytes.
    ///
    /// Uses named encoding (field names preserved in the output) to ensure
    /// compatibility with types that use `serde_json::Value` as an intermediate
    /// deserialization step (e.g., `FlowResult`).
    pub fn serialize(&self) -> Result<Bytes, rmp_serde::encode::Error> {
        let bytes = rmp_serde::to_vec_named(self)?;
        Ok(bytes.into())
    }

    /// Deserialize from MessagePack bytes.
    ///
    /// Returns an error if the checkpoint version doesn't match the current
    /// version. Callers should treat this as a recoverable error and fall
    /// back to full journal replay.
    pub fn deserialize(data: &[u8]) -> std::result::Result<Self, CheckpointDeserializeError> {
        let checkpoint: Self = rmp_serde::from_slice(data)?;
        if checkpoint.version != CHECKPOINT_VERSION {
            return Err(CheckpointDeserializeError::VersionMismatch {
                expected: CHECKPOINT_VERSION,
                actual: checkpoint.version,
            });
        }
        Ok(checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use stepflow_core::ValueExpr;
    use stepflow_core::workflow::{FlowBuilder, StepBuilder};

    fn create_test_run_state() -> RunState {
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step2".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let run_id = Uuid::now_v7();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let mut state = RunState::new(run_id, flow_id, flow, inputs, HashMap::new());
        state.initialize_all();
        state
    }

    #[test]
    fn test_checkpoint_roundtrip() {
        let run_state = create_test_run_state();
        let run_id = run_state.run_id();

        let mut runs = HashMap::new();
        runs.insert(run_id, run_state);

        let subflow_map = HashMap::new();
        let sequence = SequenceNumber::new(42);

        let checkpoint = CheckpointData::capture(&runs, &subflow_map, sequence);
        assert_eq!(checkpoint.version, CHECKPOINT_VERSION);
        assert_eq!(checkpoint.sequence, sequence);
        assert_eq!(checkpoint.runs.len(), 1);
        assert!(checkpoint.subflow_map.is_empty());

        // Serialize and deserialize
        let bytes = checkpoint.serialize().expect("serialize should succeed");
        let restored = CheckpointData::deserialize(&bytes).expect("deserialize should succeed");

        assert_eq!(restored.version, checkpoint.version);
        assert_eq!(restored.sequence, checkpoint.sequence);
        assert_eq!(restored.runs.len(), 1);
        assert_eq!(restored.runs[0].run_id, run_id);
    }

    #[test]
    fn test_checkpoint_with_completed_steps() {
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step2".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let run_id = Uuid::now_v7();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let mut state = RunState::new(run_id, flow_id, flow, inputs, HashMap::new());
        state.initialize_all();

        // Complete step1
        state.apply_event(&stepflow_state::JournalEvent::TaskCompleted {
            run_id,
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({"result": 1}))),
        });

        let mut runs = HashMap::new();
        runs.insert(run_id, state);

        let checkpoint = CheckpointData::capture(&runs, &HashMap::new(), SequenceNumber::new(5));
        let bytes = checkpoint.serialize().expect("serialize");
        let restored = CheckpointData::deserialize(&bytes).expect("deserialize");

        let item = &restored.runs[0].items[0];
        assert_eq!(item.completed, vec![0]);
        assert_eq!(item.results.len(), 1);
        assert_eq!(item.results[0].0, 0);
    }

    #[test]
    fn test_checkpoint_with_subflow_mappings() {
        let run_state = create_test_run_state();
        let run_id = run_state.run_id();

        let mut runs = HashMap::new();
        runs.insert(run_id, run_state);

        let subflow_key = Uuid::now_v7();
        let subflow_run_id = Uuid::now_v7();
        let mut subflow_map = HashMap::new();
        subflow_map.insert((run_id, 0u32, 0usize, subflow_key), subflow_run_id);

        let checkpoint = CheckpointData::capture(&runs, &subflow_map, SequenceNumber::new(10));
        assert_eq!(checkpoint.subflow_map.len(), 1);

        let bytes = checkpoint.serialize().expect("serialize");
        let restored = CheckpointData::deserialize(&bytes).expect("deserialize");

        assert_eq!(restored.subflow_map.len(), 1);
        assert_eq!(restored.subflow_map[0].parent_run_id, run_id);
        assert_eq!(restored.subflow_map[0].subflow_run_id, subflow_run_id);
    }

    #[test]
    fn test_checkpoint_restore_produces_equivalent_state() {
        // Create a flow, initialize state, complete a step, checkpoint, restore
        let flow = Arc::new(
            FlowBuilder::test_flow()
                .steps(vec![
                    StepBuilder::mock_step("step1")
                        .input(ValueExpr::Input {
                            input: Default::default(),
                        })
                        .build(),
                    StepBuilder::mock_step("step2")
                        .input(ValueExpr::Step {
                            step: "step1".to_string(),
                            path: Default::default(),
                        })
                        .build(),
                ])
                .output(ValueExpr::Step {
                    step: "step2".to_string(),
                    path: Default::default(),
                })
                .build(),
        );
        let flow_id = BlobId::from_flow(&flow).unwrap();
        let run_id = Uuid::now_v7();
        let inputs = vec![ValueRef::new(json!({"x": 1}))];

        let mut state = RunState::new(run_id, flow_id, flow.clone(), inputs, HashMap::new());
        state.initialize_all();

        // Complete step1
        state.apply_event(&stepflow_state::JournalEvent::TaskCompleted {
            run_id,
            item_index: 0,
            step_index: 0,
            result: FlowResult::Success(ValueRef::new(json!({"result": 1}))),
        });

        // Checkpoint the state
        let checkpoint = state.to_checkpoint();

        // Serialize → deserialize → restore
        let checkpoint_data = CheckpointData {
            version: 1,
            sequence: SequenceNumber::new(5),
            runs: vec![checkpoint],
            subflow_map: vec![],
        };
        let bytes = checkpoint_data.serialize().expect("serialize");
        let restored_data = CheckpointData::deserialize(&bytes).expect("deserialize");

        let restored =
            RunState::from_checkpoint(&restored_data.runs[0], flow).expect("checkpoint restore");

        // Compare key properties
        assert_eq!(restored.run_id(), state.run_id());
        assert_eq!(restored.is_complete(), state.is_complete());

        // Both should have step1 completed (step_index=0)
        let original_item = state.items_state().item(0);
        let restored_item = restored.items_state().item(0);
        assert!(original_item.is_completed(0));
        assert!(restored_item.is_completed(0));
        assert!(original_item.get_step_result(0).is_some());
        assert!(restored_item.get_step_result(0).is_some());

        // Both should agree on schedulable steps (step2 should be ready)
        let original_schedulable = original_item.schedulable_steps();
        let restored_schedulable = restored_item.schedulable_steps();
        assert_eq!(original_schedulable, restored_schedulable);

        // After applying the same event, both should produce the same state
        let event = stepflow_state::JournalEvent::TaskCompleted {
            run_id,
            item_index: 0,
            step_index: 1,
            result: FlowResult::Success(ValueRef::new(json!({"result": 2}))),
        };
        let mut original = state;
        original.apply_event(&event);
        let mut restored = restored;
        restored.apply_event(&event);

        assert_eq!(original.is_complete(), restored.is_complete());
    }

    #[test]
    fn test_empty_checkpoint() {
        let checkpoint = CheckpointData {
            version: CHECKPOINT_VERSION,
            sequence: SequenceNumber::new(0),
            runs: Vec::new(),
            subflow_map: Vec::new(),
        };

        let bytes = checkpoint.serialize().expect("serialize");
        let restored = CheckpointData::deserialize(&bytes).expect("deserialize");

        assert_eq!(restored.version, CHECKPOINT_VERSION);
        assert!(restored.runs.is_empty());
        assert!(restored.subflow_map.is_empty());
    }

    #[test]
    fn test_deserialize_version_mismatch() {
        let checkpoint = CheckpointData {
            version: 999, // Wrong version
            sequence: SequenceNumber::new(0),
            runs: Vec::new(),
            subflow_map: Vec::new(),
        };

        let bytes = rmp_serde::to_vec_named(&checkpoint).expect("serialize");
        let err = CheckpointData::deserialize(&bytes).unwrap_err();
        assert!(
            matches!(
                err,
                CheckpointDeserializeError::VersionMismatch {
                    expected: 1,
                    actual: 999
                }
            ),
            "Expected VersionMismatch error, got: {err}"
        );
    }

    #[test]
    fn test_deserialize_corrupt_data() {
        let err = CheckpointData::deserialize(&[0xFF, 0xFF, 0xFF]).unwrap_err();
        assert!(
            matches!(err, CheckpointDeserializeError::Decode(_)),
            "Expected Decode error, got: {err}"
        );
    }

    #[test]
    fn test_capture_deterministic_ordering() {
        // Create multiple runs and verify capture produces consistent ordering.
        let mut runs = HashMap::new();
        for _ in 0..5 {
            let state = create_test_run_state();
            runs.insert(state.run_id(), state);
        }

        let subflow_map = HashMap::new();
        let cp1 = CheckpointData::capture(&runs, &subflow_map, SequenceNumber::new(1));
        let cp2 = CheckpointData::capture(&runs, &subflow_map, SequenceNumber::new(1));

        let ids1: Vec<_> = cp1.runs.iter().map(|r| r.run_id).collect();
        let ids2: Vec<_> = cp2.runs.iter().map(|r| r.run_id).collect();
        assert_eq!(
            ids1, ids2,
            "Capture should produce deterministic run ordering"
        );

        // Verify runs are sorted by run_id
        for w in ids1.windows(2) {
            assert!(w[0] < w[1], "Runs should be sorted by run_id");
        }
    }
}
