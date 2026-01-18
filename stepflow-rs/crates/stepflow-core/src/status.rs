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

use serde::{Deserialize, Serialize};

use crate::workflow::StepId;

/// Status of a workflow execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionStatus {
    /// Execution is currently running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed with an error
    Failed,
    /// Execution was cancelled by user request
    Cancelled,
    /// Execution is paused (debug mode)
    Paused,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Running => "running",
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
            ExecutionStatus::Cancelled => "cancelled",
            ExecutionStatus::Paused => "paused",
        }
    }
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Status of an individual step within a workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    /// Step is waiting for dependencies to complete
    Blocked,
    /// Step is ready to be executed
    Runnable,
    /// Step is currently executing
    Running,
    /// Step has been executed successfully
    Completed,
    /// Step was skipped due to conditions
    Skipped,
    /// Step failed with an error
    Failed,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Blocked => write!(f, "blocked"),
            StepStatus::Runnable => write!(f, "runnable"),
            StepStatus::Running => write!(f, "running"),
            StepStatus::Completed => write!(f, "completed"),
            StepStatus::Skipped => write!(f, "skipped"),
            StepStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Detailed step execution information combining status and context
#[derive(Debug, Clone, PartialEq)]
pub struct StepExecution {
    /// Step identifier (index + name)
    pub step_id: StepId,
    /// Component name/URL that this step executes
    pub component: String,
    /// Current status of the step
    pub status: StepStatus,
}

impl StepExecution {
    pub fn new(step_id: StepId, component: String, status: StepStatus) -> Self {
        Self {
            step_id,
            component,
            status,
        }
    }

    /// Get the step index.
    pub fn step_index(&self) -> usize {
        self.step_id.index()
    }

    /// Get the step name.
    pub fn step_name(&self) -> &str {
        self.step_id.name()
    }
}

// Custom serialization to maintain backward compatibility
impl Serialize for StepExecution {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct as _;
        let mut state = serializer.serialize_struct("StepExecution", 4)?;
        state.serialize_field("step_index", &self.step_id.index())?;
        state.serialize_field("step_id", &self.step_id.name())?;
        state.serialize_field("component", &self.component)?;
        state.serialize_field("status", &self.status)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for StepExecution {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StepExecutionData {
            step_index: usize,
            step_id: String,
            component: String,
            status: StepStatus,
        }
        let data = StepExecutionData::deserialize(deserializer)?;
        Ok(Self {
            step_id: StepId::new(data.step_id, data.step_index),
            component: data.component,
            status: data.status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_status_display() {
        assert_eq!(ExecutionStatus::Running.to_string(), "running");
        assert_eq!(ExecutionStatus::Completed.to_string(), "completed");
        assert_eq!(ExecutionStatus::Failed.to_string(), "failed");
        assert_eq!(ExecutionStatus::Cancelled.to_string(), "cancelled");
        assert_eq!(ExecutionStatus::Paused.to_string(), "paused");
    }

    #[test]
    fn test_step_status_display() {
        assert_eq!(StepStatus::Blocked.to_string(), "blocked");
        assert_eq!(StepStatus::Runnable.to_string(), "runnable");
        assert_eq!(StepStatus::Running.to_string(), "running");
        assert_eq!(StepStatus::Completed.to_string(), "completed");
        assert_eq!(StepStatus::Skipped.to_string(), "skipped");
        assert_eq!(StepStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_execution_status_serialization() {
        let status = ExecutionStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");

        let deserialized: ExecutionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }

    #[test]
    fn test_step_status_serialization() {
        let status = StepStatus::Completed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"completed\"");

        let deserialized: StepStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }

    #[test]
    fn test_step_execution_creation() {
        let step_id = StepId::new("test_step".to_string(), 0);
        let step_exec =
            StepExecution::new(step_id, "test_component".to_string(), StepStatus::Completed);

        assert_eq!(step_exec.step_index(), 0);
        assert_eq!(step_exec.step_name(), "test_step");
        assert_eq!(step_exec.component, "test_component");
        assert_eq!(step_exec.status, StepStatus::Completed);
    }
}
