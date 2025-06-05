use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult, blob::BlobId, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef,
};
use stepflow_plugin::ExecutionContext;

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for creating blobs from JSON data.
pub struct PutBlobComponent;

impl PutBlobComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PutBlobComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for the put_blob component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct PutBlobInput {
    /// The JSON data to store as a blob
    data: serde_json::Value,
}

/// Output from the put_blob component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct PutBlobOutput {
    /// The blob ID for the stored data
    blob_id: String,
}

impl BuiltinComponent for PutBlobComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<PutBlobInput>();
        let output_schema = SchemaRef::for_type::<PutBlobOutput>();

        Ok(ComponentInfo {
            input_schema,
            output_schema,
            description: Some("Store JSON data as a blob and return its content-addressable ID".to_string()),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let input: PutBlobInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let data_ref = ValueRef::new(input.data);

        // Create the blob through the execution context
        let blob_id = context
            .state_store()
            .put_blob(data_ref)
            .await
            .change_context(BuiltinError::Internal)?;

        let output = PutBlobOutput {
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
            description: Some("Retrieve JSON data from a blob using its ID".to_string()),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
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
        let component = PutBlobComponent::new();
        let test_data = json!({"message": "Hello, blobs!", "number": 42});

        let input = PutBlobInput {
            data: test_data.clone(),
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success { result } => {
                let output: PutBlobOutput =
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
        let mock = MockContext::new();

        // First, store the blob
        let blob_id = mock
            .execution_context()
            .state_store()
            .put_blob(ValueRef::new(test_data.clone()))
            .await
            .unwrap();

        let input = GetBlobInput {
            blob_id: blob_id.as_str().to_string(),
        };

        let input_value = serde_json::to_value(input).unwrap();

        let result = component
            .execute(mock.execution_context(), input_value.into())
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
        let create_component = PutBlobComponent::new();
        let get_component = GetBlobComponent::new();

        let test_data = json!({"roundtrip": "test", "complex": {"nested": [1, 2, 3]}});

        // Use the same mock context for both operations to share state
        let mock = MockContext::new();

        // Create blob
        let create_input = PutBlobInput {
            data: test_data.clone(),
        };
        let create_input_value = serde_json::to_value(create_input).unwrap();

        let create_result = create_component
            .execute(mock.execution_context(), create_input_value.into())
            .await
            .unwrap();

        let blob_id = match create_result {
            FlowResult::Success { result } => {
                let output: PutBlobOutput =
                    serde_json::from_value(result.as_ref().clone()).unwrap();
                output.blob_id
            }
            _ => panic!("Expected success result from create"),
        };

        // Get blob using the same mock context (shares the same state store)
        let get_input = GetBlobInput { blob_id };
        let get_input_value = serde_json::to_value(get_input).unwrap();

        let get_result = get_component
            .execute(mock.execution_context(), get_input_value.into())
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
        let mock = MockContext::new();

        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await;

        // Should return an error for invalid blob ID
        assert!(result.is_err());
    }
}
