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

//! Per-component consecutive failure tracking (poison pill detection).
//!
//! Tracks consecutive task failures per component path. When a component
//! exceeds the configured threshold, it is flagged as unhealthy. A single
//! success resets the counter.
//!
//! This is observational only — unhealthy status is exposed via metrics and
//! the API but does not block task dispatch.

use dashmap::DashMap;

/// Default number of consecutive failures before a component is flagged unhealthy.
const DEFAULT_UNHEALTHY_THRESHOLD: u32 = 5;

/// Tracks per-component consecutive failure counts for poison pill detection.
pub struct ComponentHealthTracker {
    /// Component path → consecutive failure count.
    failures: DashMap<String, u32>,
    /// Number of consecutive failures before a component is flagged unhealthy.
    threshold: u32,
}

impl ComponentHealthTracker {
    /// Create a new tracker with the default threshold.
    pub fn new() -> Self {
        Self {
            failures: DashMap::new(),
            threshold: DEFAULT_UNHEALTHY_THRESHOLD,
        }
    }

    /// Create a new tracker with a custom threshold.
    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            failures: DashMap::new(),
            threshold,
        }
    }

    /// Record a successful task completion for a component.
    ///
    /// Resets the consecutive failure counter to zero.
    pub fn record_success(&self, component: &str) {
        self.failures.remove(component);
    }

    /// Record a failed task completion for a component.
    ///
    /// Returns `true` if the component just crossed the unhealthy threshold
    /// (i.e., was healthy before this failure, unhealthy after).
    pub fn record_failure(&self, component: &str) -> bool {
        let mut entry = self.failures.entry(component.to_string()).or_insert(0);
        *entry += 1;
        *entry == self.threshold
    }

    /// Check whether a component is currently flagged as unhealthy.
    pub fn is_unhealthy(&self, component: &str) -> bool {
        self.failures
            .get(component)
            .is_some_and(|count| *count >= self.threshold)
    }

    /// Reset a component's failure counter (manual recovery).
    pub fn reset(&self, component: &str) {
        self.failures.remove(component);
    }

    /// Get the current consecutive failure count for a component.
    pub fn failure_count(&self, component: &str) -> u32 {
        self.failures.get(component).map_or(0, |c| *c)
    }
}

impl Default for ComponentHealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_healthy_by_default() {
        let health = ComponentHealthTracker::new();
        assert!(!health.is_unhealthy("/python/my_func"));
        assert_eq!(health.failure_count("/python/my_func"), 0);
    }

    #[test]
    fn test_failures_below_threshold_stay_healthy() {
        let health = ComponentHealthTracker::with_threshold(3);
        assert!(!health.record_failure("/python/my_func")); // 1
        assert!(!health.record_failure("/python/my_func")); // 2
        assert!(!health.is_unhealthy("/python/my_func"));
        assert_eq!(health.failure_count("/python/my_func"), 2);
    }

    #[test]
    fn test_crossing_threshold_returns_true_once() {
        let health = ComponentHealthTracker::with_threshold(3);
        assert!(!health.record_failure("/python/f")); // 1
        assert!(!health.record_failure("/python/f")); // 2
        assert!(health.record_failure("/python/f")); // 3 — crosses threshold
        assert!(!health.record_failure("/python/f")); // 4 — already unhealthy
        assert!(health.is_unhealthy("/python/f"));
    }

    #[test]
    fn test_success_resets_counter() {
        let health = ComponentHealthTracker::with_threshold(3);
        health.record_failure("/python/f");
        health.record_failure("/python/f");
        health.record_success("/python/f");
        assert!(!health.is_unhealthy("/python/f"));
        assert_eq!(health.failure_count("/python/f"), 0);
    }

    #[test]
    fn test_components_are_independent() {
        let health = ComponentHealthTracker::with_threshold(2);
        health.record_failure("/python/a");
        health.record_failure("/python/b");
        assert!(!health.is_unhealthy("/python/a"));
        assert!(!health.is_unhealthy("/python/b"));
        health.record_failure("/python/a"); // 2 — threshold
        assert!(health.is_unhealthy("/python/a"));
        assert!(!health.is_unhealthy("/python/b"));
    }

    #[test]
    fn test_manual_reset() {
        let health = ComponentHealthTracker::with_threshold(2);
        health.record_failure("/python/f");
        health.record_failure("/python/f");
        assert!(health.is_unhealthy("/python/f"));
        health.reset("/python/f");
        assert!(!health.is_unhealthy("/python/f"));
    }
}
