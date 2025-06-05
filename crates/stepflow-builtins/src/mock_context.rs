use std::{pin::Pin, sync::Arc};
use stepflow_core::{
    FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::ExecutionContext;
use stepflow_state::InMemoryStateStore;
use uuid::Uuid;

/// A mock execution context for testing built-in components.
///
/// This provides a way to create ExecutionContext instances for testing
/// built-in components without requiring complex workflow execution.
pub struct MockContext {
    executor: Arc<MockExecutor>,
}

impl MockContext {
    /// Create a new mock context with a random execution ID.
    pub fn new() -> Self {
        Self {
            executor: Arc::new(MockExecutor {
                state_store: Arc::new(InMemoryStateStore::new()),
            }),
        }
    }

    /// Get an execution context for testing from this mock context.
    pub fn execution_context(&self) -> ExecutionContext {
        ExecutionContext::new(self.executor.clone(), Uuid::new_v4())
    }
}

impl Default for MockContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock executor implementation for testing.
struct MockExecutor {
    state_store: Arc<dyn stepflow_state::StateStore>,
}

impl stepflow_plugin::Context for MockExecutor {
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

    fn state_store(&self) -> &Arc<dyn stepflow_state::StateStore> {
        &self.state_store
    }
}
