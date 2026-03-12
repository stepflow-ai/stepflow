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

//! Heartbeat tracking for executing tasks.
//!
//! [`HeartbeatTracker`] maintains a set of task IDs that have sent a heartbeat
//! in the current scan cycle.  The heartbeat scanner checks all executing
//! tasks against the set each cycle and calls [`reset`] to start a new one.
//!
//! [`reset`]: HeartbeatTracker::reset

use dashmap::DashSet;

/// Tracks which tasks have sent a heartbeat in the current scan cycle.
///
/// Tasks insert their ID on each heartbeat; the scanner checks all executing
/// tasks against the set and resets it at the start of each new cycle.
pub(super) struct HeartbeatTracker {
    seen: DashSet<String>,
}

impl HeartbeatTracker {
    pub(super) fn new() -> Self {
        Self {
            seen: DashSet::new(),
        }
    }

    /// Record a heartbeat from `id`.
    pub(super) fn record(&self, id: &str) {
        self.seen.insert(id.to_string());
    }

    /// Remove `id` (called when a task completes or is removed).
    pub(super) fn remove(&self, id: &str) {
        self.seen.remove(id);
    }

    /// Returns `true` if `id` sent a heartbeat in the current cycle.
    pub(super) fn was_seen(&self, id: &str) -> bool {
        self.seen.contains(id)
    }

    /// Clear all entries to begin a new scan cycle.
    pub(super) fn reset(&self) {
        self.seen.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_was_seen() {
        let tracker = HeartbeatTracker::new();
        assert!(!tracker.was_seen("task-1"));
        tracker.record("task-1");
        assert!(tracker.was_seen("task-1"));
    }

    #[test]
    fn test_remove_clears_entry() {
        let tracker = HeartbeatTracker::new();
        tracker.record("task-1");
        tracker.remove("task-1");
        assert!(!tracker.was_seen("task-1"));
    }

    #[test]
    fn test_reset_clears_all() {
        let tracker = HeartbeatTracker::new();
        tracker.record("task-1");
        tracker.record("task-2");
        tracker.reset();
        assert!(!tracker.was_seen("task-1"));
        assert!(!tracker.was_seen("task-2"));
    }

    #[test]
    fn test_remove_nonexistent_is_noop() {
        let tracker = HeartbeatTracker::new();
        tracker.remove("nonexistent"); // should not panic
        assert!(!tracker.was_seen("nonexistent"));
    }
}
