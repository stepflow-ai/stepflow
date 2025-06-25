use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Status of a workflow execution
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct StepExecution {
    /// Step index in the workflow
    pub step_index: usize,
    /// Step ID (if provided)
    pub step_id: String,
    /// Component name/URL that this step executes
    pub component: String,
    /// Current status of the step
    pub status: StepStatus,
}

impl StepExecution {
    pub fn new(step_index: usize, step_id: String, component: String, status: StepStatus) -> Self {
        Self {
            step_index,
            step_id,
            component,
            status,
        }
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
        let step_exec = StepExecution::new(
            0,
            "test_step".to_string(),
            "test_component".to_string(),
            StepStatus::Completed,
        );

        assert_eq!(step_exec.step_index, 0);
        assert_eq!(step_exec.step_id, "test_step");
        assert_eq!(step_exec.component, "test_component");
        assert_eq!(step_exec.status, StepStatus::Completed);
    }
}
