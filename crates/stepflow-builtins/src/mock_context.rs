use std::{pin::Pin, sync::Arc};
use stepflow_core::{
    FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::ExecutionContext;
use uuid::Uuid;

/// A mock execution context for testing built-in components.
///
/// This provides default implementations of the ExecutionContext trait
/// that are suitable for testing most built-in components that don't
/// require complex workflow execution.
pub struct MockContext {
    id: Uuid,
}

impl MockContext {
    /// Create a new mock context with a random execution ID.
    pub fn new() -> Self {
        Self { id: Uuid::new_v4() }
    }

    /// Create a new mock context with a specific execution ID.
    pub fn with_id(id: Uuid) -> Self {
        Self { id }
    }
}

impl Default for MockContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionContext for MockContext {
    fn execution_id(&self) -> Uuid {
        self.id
    }

    fn submit_flow(
        &self,
        _flow: Arc<Flow>,
        _input: ValueRef,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<Uuid>> + Send + '_>> {
        Box::pin(async { Ok(Uuid::new_v4()) })
    }

    fn flow_result(
        &self,
        _execution_id: Uuid,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<FlowResult>> + Send + '_>>
    {
        Box::pin(async {
            // Return a simple success result for testing
            let result = serde_json::json!({"message": "Hello from nested flow"});
            Ok(FlowResult::Success {
                result: ValueRef::new(result),
            })
        })
    }
}
