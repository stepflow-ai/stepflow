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

//! Debug history and state derivation for interactive workflow debugging.
//!
//! This module provides [`DebugHistory`] for deriving execution state from debug events.
//! Events are persisted in the [`StateStore`](stepflow_state::StateStore) and this struct
//! provides a view of the derived state.
//!
//! # Design
//!
//! The debugger maintains:
//! 1. **Event history**: What has happened (step queued, started, completed, failed, etc.)
//!    - Persisted via `StateStore::append_debug_event()` and retrieved via `get_debug_events()`
//! 2. **Derived state**: Computed from events (queued steps, completed steps, pending action)
//!
//! This enables interactive debugging where each action can be inspected
//! before proceeding, similar to GDB-style debugging.

use std::collections::HashSet;

// Re-export the shared DTOs for convenience
pub use stepflow_dtos::{DebugEvent, PendingAction};

/// Debug history and state derivation for a debug session.
///
/// This struct is constructed from a list of debug events and provides
/// methods to query the derived state (queued steps, completed steps, etc.).
///
/// Events are stored externally in the `StateStore` - this struct only
/// provides a view/analysis of those events.
#[derive(Debug, Clone)]
pub struct DebugHistory {
    /// The events (owned, can be empty for new sessions).
    events: Vec<DebugEvent>,
}

impl DebugHistory {
    /// Create a new debug history from a list of events.
    ///
    /// Events should be in newest-first order (as returned by `StateStore::get_debug_events`).
    pub fn from_events(events: Vec<DebugEvent>) -> Self {
        Self { events }
    }

    /// Create an empty debug history (for new sessions).
    pub fn new() -> Self {
        Self { events: vec![] }
    }

    /// Get the events (newest first).
    pub fn events(&self) -> &[DebugEvent] {
        &self.events
    }

    /// Get the number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if there are no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get the set of step indices that have been completed.
    pub fn completed_step_indices(&self) -> HashSet<usize> {
        self.events
            .iter()
            .filter_map(|e| match e {
                DebugEvent::StepCompleted { step_index, .. } => Some(*step_index),
                _ => None,
            })
            .collect()
    }

    /// Get the set of step IDs that have been queued but not completed.
    ///
    /// These are steps that need to be added to the needed set during recovery.
    pub fn pending_step_ids(&self) -> Vec<String> {
        let completed = self.completed_step_indices();

        self.events
            .iter()
            .filter_map(|e| match e {
                DebugEvent::StepQueued {
                    step_index,
                    step_id,
                } => {
                    if !completed.contains(step_index) {
                        Some(step_id.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    /// Check if a specific step has been completed.
    pub fn is_step_completed(&self, step_index: usize) -> bool {
        self.events.iter().any(
            |e| matches!(e, DebugEvent::StepCompleted { step_index: idx, .. } if *idx == step_index),
        )
    }

    /// Check if a specific step has been queued.
    pub fn is_step_queued(&self, step_index: usize) -> bool {
        self.events.iter().any(
            |e| matches!(e, DebugEvent::StepQueued { step_index: idx, .. } if *idx == step_index),
        )
    }

    /// Derive the pending action from the events.
    ///
    /// Returns:
    /// - `Complete` if there's a `RunCompleted` event
    /// - `ExecuteStep` if there are queued but not completed steps
    /// - `AwaitingInput` otherwise
    pub fn pending_action(&self) -> PendingAction {
        // Check for run completion first
        if self
            .events
            .iter()
            .any(|e| matches!(e, DebugEvent::RunCompleted { .. }))
        {
            return PendingAction::Complete;
        }

        // Find the first queued but not completed step
        let completed = self.completed_step_indices();

        for event in &self.events {
            if let DebugEvent::StepQueued {
                step_index,
                step_id,
            } = event
                && !completed.contains(step_index)
            {
                return PendingAction::execute_step(*step_index, step_id);
            }
        }

        PendingAction::AwaitingInput
    }
}

impl Default for DebugHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::{FlowResult, values::ValueRef};
    use uuid::Uuid;

    #[test]
    fn test_debug_history_new() {
        let history = DebugHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        assert!(history.pending_action().is_awaiting_input());
    }

    #[test]
    fn test_debug_history_from_events() {
        let events = vec![
            DebugEvent::step_queued(0, "step1"),
            DebugEvent::step_queued(1, "step2"),
        ];
        let history = DebugHistory::from_events(events);

        assert_eq!(history.len(), 2);
        assert!(!history.is_empty());
    }

    #[test]
    fn test_completed_step_indices() {
        let result = FlowResult::Success(ValueRef::new(json!({})));
        let events = vec![
            DebugEvent::step_completed(0, "step1", result.clone()),
            DebugEvent::step_queued(1, "step2"),
            DebugEvent::step_completed(2, "step3", result),
        ];
        let history = DebugHistory::from_events(events);

        let completed = history.completed_step_indices();
        assert!(completed.contains(&0));
        assert!(!completed.contains(&1));
        assert!(completed.contains(&2));
    }

    #[test]
    fn test_pending_step_ids() {
        let result = FlowResult::Success(ValueRef::new(json!({})));
        let events = vec![
            DebugEvent::step_queued(0, "step1"),
            DebugEvent::step_completed(0, "step1", result),
            DebugEvent::step_queued(1, "step2"), // Not completed
        ];
        let history = DebugHistory::from_events(events);

        let pending = history.pending_step_ids();
        assert!(!pending.contains(&"step1".to_string())); // Completed
        assert!(pending.contains(&"step2".to_string())); // Still pending
    }

    #[test]
    fn test_is_step_completed() {
        let result = FlowResult::Success(ValueRef::new(json!({})));
        let events = vec![
            DebugEvent::step_queued(0, "step1"),
            DebugEvent::step_completed(0, "step1", result),
        ];
        let history = DebugHistory::from_events(events);

        assert!(history.is_step_completed(0));
        assert!(!history.is_step_completed(1));
    }

    #[test]
    fn test_is_step_queued() {
        let events = vec![DebugEvent::step_queued(0, "step1")];
        let history = DebugHistory::from_events(events);

        assert!(history.is_step_queued(0));
        assert!(!history.is_step_queued(1));
    }

    #[test]
    fn test_pending_action_awaiting_input() {
        let history = DebugHistory::new();
        assert!(history.pending_action().is_awaiting_input());
    }

    #[test]
    fn test_pending_action_execute_step() {
        let events = vec![DebugEvent::step_queued(0, "step1")];
        let history = DebugHistory::from_events(events);

        let action = history.pending_action();
        assert!(action.is_execute_step());
        if let PendingAction::ExecuteStep {
            step_index,
            step_id,
        } = action
        {
            assert_eq!(step_index, 0);
            assert_eq!(step_id, "step1");
        }
    }

    #[test]
    fn test_pending_action_complete() {
        let events = vec![DebugEvent::run_completed(Uuid::nil(), true)];
        let history = DebugHistory::from_events(events);

        assert!(history.pending_action().is_complete());
    }

    #[test]
    fn test_pending_action_after_partial_completion() {
        let result = FlowResult::Success(ValueRef::new(json!({})));
        let events = vec![
            // Oldest first in time, but events are stored newest-first
            DebugEvent::step_queued(1, "step2"), // Most recent
            DebugEvent::step_completed(0, "step1", result),
            DebugEvent::step_queued(0, "step1"),
        ];
        let history = DebugHistory::from_events(events);

        // step2 is queued but not completed, so pending action should be execute_step
        let action = history.pending_action();
        assert!(action.is_execute_step());
        if let PendingAction::ExecuteStep { step_id, .. } = action {
            assert_eq!(step_id, "step2");
        }
    }

    #[test]
    fn test_debug_event_serialization() {
        let event = DebugEvent::step_queued(0, "step1");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("step_queued"));
        assert!(json.contains("step1"));

        let deserialized: DebugEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_debug_event_step_started() {
        let input = ValueRef::new(json!({"key": "value"}));
        let event = DebugEvent::step_started(1, "step2", input.clone());

        if let DebugEvent::StepStarted {
            step_index,
            step_id,
            input: event_input,
        } = event
        {
            assert_eq!(step_index, 1);
            assert_eq!(step_id, "step2");
            assert_eq!(event_input, input);
        } else {
            panic!("Expected StepStarted event");
        }
    }

    #[test]
    fn test_debug_event_step_completed() {
        let result = FlowResult::Success(ValueRef::new(json!({"output": 42})));
        let event = DebugEvent::step_completed(2, "step3", result.clone());

        if let DebugEvent::StepCompleted {
            step_index,
            step_id,
            result: event_result,
        } = event
        {
            assert_eq!(step_index, 2);
            assert_eq!(step_id, "step3");
            assert_eq!(event_result, result);
        } else {
            panic!("Expected StepCompleted event");
        }
    }

    #[test]
    fn test_debug_event_accessors() {
        let event = DebugEvent::step_queued(5, "my_step");
        assert_eq!(event.step_index(), Some(5));
        assert_eq!(event.step_id(), Some("my_step"));

        let run_event = DebugEvent::run_completed(Uuid::nil(), true);
        assert_eq!(run_event.step_index(), None);
        assert_eq!(run_event.step_id(), None);
    }

    #[test]
    fn test_pending_action_serialization() {
        let action = PendingAction::execute_step(0, "step1");
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("execute_step"));

        let deserialized: PendingAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_pending_action_checks() {
        assert!(PendingAction::Complete.is_complete());
        assert!(!PendingAction::Complete.is_execute_step());
        assert!(!PendingAction::Complete.is_awaiting_input());

        assert!(PendingAction::execute_step(0, "s").is_execute_step());
        assert!(!PendingAction::execute_step(0, "s").is_complete());

        assert!(PendingAction::AwaitingInput.is_awaiting_input());
        assert!(!PendingAction::AwaitingInput.is_complete());
    }
}
