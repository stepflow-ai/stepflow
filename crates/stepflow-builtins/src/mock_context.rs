use std::{pin::Pin, sync::Arc};
use stepflow_core::{
    FlowResult,
    blob::BlobId,
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

    fn create_blob(
        &self,
        data: ValueRef,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<BlobId>> + Send + '_>>
    {
        Box::pin(async move {
            // For testing, just create a blob ID from the content
            BlobId::from_content(&data)
                .map_err(|_e| stepflow_plugin::PluginError::UdfExecution.into())
        })
    }

    fn get_blob(
        &self,
        _blob_id: &BlobId,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<ValueRef>> + Send + '_>>
    {
        Box::pin(async {
            // For testing, return mock data
            let result = serde_json::json!({"mock": "blob data"});
            Ok(ValueRef::new(result))
        })
    }
}
