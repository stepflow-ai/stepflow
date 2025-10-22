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

//! Run diagnostic context for workflow executions
//!
//! This module provides thread-local storage for run_id and step_id that gets
//! automatically injected into all log records via a custom diagnostic.
//!
//! - `run_id`: Set once per workflow execution (rarely changes)
//! - `step_id`: Set/cleared frequently as we enter/exit steps

use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunInfo {
    flow_id: String,
    run_id: String,
}

thread_local! {
    static RUN_INFO: RefCell<Option<RunInfo>> = const { RefCell::new(None) };
    static STEP_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// RAII guard that sets run_id on creation and clears it on drop
pub struct RunInfoGuard {
    _private: (),
}

impl RunInfoGuard {
    /// Create a new guard and set the run_id for the current thread
    pub fn new(flow_id: impl Into<String>, run_id: impl Into<String>) -> Self {
        let run_info = RunInfo {
            flow_id: flow_id.into(),
            run_id: run_id.into(),
        };
        RUN_INFO.with(|r| {
            *r.borrow_mut() = Some(run_info);
        });
        Self { _private: () }
    }
}

impl Drop for RunInfoGuard {
    fn drop(&mut self) {
        RUN_INFO.with(|r| {
            *r.borrow_mut() = None;
        });
    }
}

/// RAII guard that sets step_id on creation and clears it on drop
pub struct StepIdGuard {
    _private: (),
}

impl StepIdGuard {
    /// Create a new guard and set the step_id for the current thread
    pub fn new(step_id: impl Into<String>) -> Self {
        STEP_ID.with(|s| {
            *s.borrow_mut() = Some(step_id.into());
        });
        Self { _private: () }
    }
}

impl Drop for StepIdGuard {
    fn drop(&mut self) {
        STEP_ID.with(|s| {
            *s.borrow_mut() = None;
        });
    }
}

/// Get the current run_id if set
pub fn get_run_info() -> Option<RunInfo> {
    RUN_INFO.with(|r| r.borrow().clone())
}

/// Get the current step_id if set
pub fn get_step_id() -> Option<String> {
    STEP_ID.with(|s| s.borrow().clone())
}

/// Custom diagnostic that injects run_id and step_id into logs
#[derive(Debug, Default)]
pub struct RunDiagnostic;

impl logforth::diagnostic::Diagnostic for RunDiagnostic {
    fn visit(&self, visitor: &mut dyn logforth::kv::Visitor) -> Result<(), logforth::Error> {
        use logforth::kv::{Key, Value};

        if let Some(run_info) = get_run_info() {
            visitor.visit(Key::new("run_id"), Value::from_str(&run_info.run_id))?;
            visitor.visit(Key::new("flow_id"), Value::from_display(&run_info.flow_id))?;
        }

        if let Some(step_id) = get_step_id() {
            visitor.visit(Key::new("step_id"), Value::from_display(&step_id))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_info_guard() {
        // Initially no run_id
        assert!(get_run_info().is_none());

        {
            let _guard = RunInfoGuard::new("flow-1", "run-123");

            // run_id is set inside guard scope
            assert_eq!(get_run_info(), Some(RunInfo {
                flow_id: "flow-1".to_string(), 
                run_id: "run-123".to_string(),
            }));
        }

        // run_id is cleared when guard drops
        assert!(get_run_info().is_none());
    }

    #[test]
    fn test_step_id_guard() {
        // Initially no step_id
        assert!(get_step_id().is_none());

        {
            let _guard = StepIdGuard::new("step1");

            // step_id is set inside guard scope
            assert_eq!(get_step_id(), Some("step1".to_string()));
        }

        // step_id is cleared when guard drops
        assert!(get_step_id().is_none());
    }

    #[test]
    fn test_nested_guards() {
        // Initially no context
        assert!(get_run_info().is_none());
        assert!(get_step_id().is_none());

        {
            let _run_guard = RunInfoGuard::new("flow-1", "run-456");
            assert_eq!(get_run_info(), Some(RunInfo {
                flow_id: "flow-1".to_string(),
                run_id: "run-456".to_string()
            }));
            assert!(get_step_id().is_none());

            {
                let _step_guard = StepIdGuard::new("step1");
                assert_eq!(get_run_info(), Some(RunInfo {
                    flow_id: "flow-1".to_string(),
                    run_id: "run-456".to_string()
                }));
                assert_eq!(get_step_id(), Some("step1".to_string()));
            }

            // step_id cleared, run_id still set
            assert_eq!(get_run_info(), Some(RunInfo {
                flow_id: "flow-1".to_string(),
                run_id: "run-456".to_string()
            }));
            assert!(get_step_id().is_none());

            {
                let _step_guard = StepIdGuard::new("step2");
                assert_eq!(get_run_info(), Some(RunInfo {
                    flow_id: "flow-1".to_string(),
                    run_id: "run-456".to_string()
                }));
                assert_eq!(get_step_id(), Some("step2".to_string()));
            }

            assert_eq!(get_run_info(), Some(RunInfo {
                flow_id: "flow-1".to_string(),
                run_id: "run-456".to_string()
            }));
            assert!(get_step_id().is_none());
        }

        // All cleared when run guard drops
        assert!(get_run_info().is_none());
        assert!(get_step_id().is_none());
    }
}
