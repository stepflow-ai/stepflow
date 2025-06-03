use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::{
    FlowResult, blob::BlobId, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef,
};
use stepflow_plugin::ExecutionContext;

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for creating blobs from JSON data.
pub struct CreateBlobComponent;

impl CreateBlobComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CreateBlobComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for the create_blob component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct CreateBlobInput {
    /// The JSON data to store as a blob
    data: serde_json::Value,
}

/// Output from the create_blob component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct CreateBlobOutput {
    /// The blob ID for the stored data
    blob_id: String,
}

impl BuiltinComponent for CreateBlobComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<CreateBlobInput>();
        let output_schema = SchemaRef::for_type::<CreateBlobOutput>();

        Ok(ComponentInfo {
            input_schema,
            output_schema,
        })
    }

    async fn execute(
        &self,
        context: Arc<dyn ExecutionContext>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let input: CreateBlobInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let data_ref = ValueRef::new(input.data);

        // Create the blob through the execution context
        let blob_id = context
            .state_store()
            .put_blob(data_ref)
            .await
            .change_context(BuiltinError::Internal)?;

        let output = CreateBlobOutput {
            blob_id: blob_id.as_str().to_string(),
        };

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success {
            result: ValueRef::new(output_value),
        })
    }
}

/// Component for retrieving blob data by ID.
pub struct GetBlobComponent;

impl GetBlobComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GetBlobComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for the get_blob component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct GetBlobInput {
    /// The blob ID to retrieve
    blob_id: String,
}

/// Output from the get_blob component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct GetBlobOutput {
    /// The JSON data stored in the blob
    data: serde_json::Value,
}

impl BuiltinComponent for GetBlobComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<GetBlobInput>();
        let output_schema = SchemaRef::for_type::<GetBlobOutput>();

        Ok(ComponentInfo {
            input_schema,
            output_schema,
        })
    }

    async fn execute(
        &self,
        context: Arc<dyn ExecutionContext>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let input: GetBlobInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        // Parse the blob ID
        let blob_id = BlobId::new(input.blob_id).map_err(|e| {
            error_stack::report!(BuiltinError::InvalidInput)
                .attach_printable(format!("Invalid blob ID: {}", e))
        })?;

        // Retrieve the blob through the execution context
        let data_ref = context
            .state_store()
            .get_blob(&blob_id)
            .await
            .change_context(BuiltinError::Internal)?;

        let output = GetBlobOutput {
            data: data_ref.as_ref().clone(),
        };

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success {
            result: ValueRef::new(output_value),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;
    use serde_json::json;

    #[tokio::test]
    async fn test_create_blob_component() {
        let component = CreateBlobComponent::new();
        let test_data = json!({"message": "Hello, blobs!", "number": 42});

        let input = CreateBlobInput {
            data: test_data.clone(),
        };

        let input_value = serde_json::to_value(input).unwrap();
        let context = MockContext::new_execution_context();

        let result = component
            .execute(context, input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
                let output: CreateBlobOutput =
                    serde_json::from_value(result.as_ref().clone()).unwrap();

                // Blob ID should be a valid SHA-256 hash (64 hex characters)
                assert_eq!(output.blob_id.len(), 64);
                assert!(output.blob_id.chars().all(|c| c.is_ascii_hexdigit()));
            }
            _ => panic!("Expected success result"),
        }
    }

    #[tokio::test]
    async fn test_get_blob_component() {
        let component = GetBlobComponent::new();

        // Create a valid blob ID from test data
        let test_data = json!({"retrieved": "data", "value": 123});
        let context = MockContext::new_execution_context();

        // First, store the blob
        let blob_id = context
            .state_store()
            .put_blob(ValueRef::new(test_data.clone()))
            .await
            .unwrap();

        let input = GetBlobInput {
            blob_id: blob_id.as_str().to_string(),
        };

        let input_value = serde_json::to_value(input).unwrap();

        let result = component
            .execute(context, input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
                let output: GetBlobOutput =
                    serde_json::from_value(result.as_ref().clone()).unwrap();

                // Should return the actual stored data
                assert_eq!(output.data, test_data);
            }
            _ => panic!("Expected success result"),
        }
    }

    #[tokio::test]
    async fn test_blob_roundtrip() {
        let create_component = CreateBlobComponent::new();
        let get_component = GetBlobComponent::new();

        let test_data = json!({"roundtrip": "test", "complex": {"nested": [1, 2, 3]}});

        // Use the same context for both operations
        let context = MockContext::new_execution_context();

        // Create blob
        let create_input = CreateBlobInput {
            data: test_data.clone(),
        };
        let create_input_value = serde_json::to_value(create_input).unwrap();

        let create_result = create_component
            .execute(context.clone(), create_input_value.into())
            .await
            .unwrap();

        let blob_id = match create_result {
            FlowResult::Success { result } => {
                let output: CreateBlobOutput =
                    serde_json::from_value(result.as_ref().clone()).unwrap();
                output.blob_id
            }
            _ => panic!("Expected success result from create"),
        };

        // Get blob (Note: MockContext returns mock data, so this tests the component structure)
        let get_input = GetBlobInput { blob_id };
        let get_input_value = serde_json::to_value(get_input).unwrap();

        let get_result = get_component
            .execute(context, get_input_value.into())
            .await
            .unwrap();

        match get_result {
            FlowResult::Success { result } => {
                let output: GetBlobOutput =
                    serde_json::from_value(result.as_ref().clone()).unwrap();
                // Should return the actual stored data
                assert_eq!(output.data, test_data);
            }
            _ => panic!("Expected success result from get"),
        }
    }

    #[tokio::test]
    async fn test_invalid_blob_id() {
        let component = GetBlobComponent::new();

        let input = GetBlobInput {
            blob_id: "invalid_blob_id".to_string(), // Not a valid SHA-256 hash
        };

        let input_value = serde_json::to_value(input).unwrap();
        let context = MockContext::new_execution_context();

        let result = component.execute(context, input_value.into()).await;

        // Should return an error for invalid blob ID
        assert!(result.is_err());
    }
}
