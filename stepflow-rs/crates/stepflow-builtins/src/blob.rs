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

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use stepflow_core::workflow::Component;
use stepflow_core::workflow::StepId;
use stepflow_core::{
    FlowResult, blob::BlobId, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef,
};
use stepflow_plugin::RunContext;

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
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
struct PutBlobInput {
    /// The JSON data to store as a blob
    data: serde_json::Value,
    /// The type of blob to store
    blob_type: stepflow_core::BlobType,
}

/// Output from the put_blob component
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
struct PutBlobOutput {
    /// The blob ID for the stored data
    blob_id: String,
}

impl BuiltinComponent for PutBlobComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<PutBlobInput>();
        let output_schema = SchemaRef::for_type::<PutBlobOutput>();

        Ok(ComponentInfo {
            component: Component::from_string("/put_blob"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Store JSON data as a blob and return its content-addressable ID".to_string(),
            ),
        })
    }

    async fn execute(
        &self,
        run_context: &Arc<RunContext>,
        _step: Option<&StepId>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let input: PutBlobInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let data_ref = ValueRef::new(input.data);

        // DEBUG: Log what's being stored
        log::debug!("put_blob storing data: {:?}", data_ref.as_ref());

        // Create the blob through the run context
        let blob_id = run_context
            .blob_store()
            .put_blob(data_ref, input.blob_type)
            .await
            .change_context(BuiltinError::Internal)?;

        log::debug!("put_blob created blob with ID: {}", blob_id.as_str());

        let output = PutBlobOutput {
            blob_id: blob_id.as_str().to_string(),
        };

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success(ValueRef::new(output_value)))
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
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
struct GetBlobInput {
    /// The blob ID to retrieve
    blob_id: String,
}

/// Output from the get_blob component
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
struct GetBlobOutput {
    /// The JSON data stored in the blob
    data: serde_json::Value,
}

impl BuiltinComponent for GetBlobComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<GetBlobInput>();
        let output_schema = SchemaRef::for_type::<GetBlobOutput>();

        Ok(ComponentInfo {
            component: Component::from_string("/get_blob"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some("Retrieve JSON data from a blob using its ID".to_string()),
        })
    }

    async fn execute(
        &self,
        run_context: &Arc<RunContext>,
        _step: Option<&StepId>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let input: GetBlobInput = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        // Parse the blob ID
        let blob_id = BlobId::new(input.blob_id).map_err(|e| {
            error_stack::report!(BuiltinError::InvalidInput)
                .attach_printable(format!("Invalid blob ID: {e}"))
        })?;

        // Retrieve the blob through the run context
        let data_ref = run_context
            .blob_store()
            .get_blob(&blob_id)
            .await
            .change_context(BuiltinError::Internal)?;

        log::debug!("get_blob retrieved data: {:?}", data_ref.data().as_ref());

        let output = GetBlobOutput {
            data: data_ref.data().as_ref().clone(),
        };

        log::debug!("get_blob output structure: {:?}", output);

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success(ValueRef::new(output_value)))
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
            blob_type: stepflow_core::BlobType::Data,
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new().await;

        let result = component
            .execute(&mock.run_context(), None, input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
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
        let mock = MockContext::new().await;

        // First, store the blob
        let blob_id = mock
            .run_context()
            .blob_store()
            .put_blob(
                ValueRef::new(test_data.clone()),
                stepflow_core::BlobType::Data,
            )
            .await
            .unwrap();

        let input = GetBlobInput {
            blob_id: blob_id.as_str().to_string(),
        };

        let input_value = serde_json::to_value(input).unwrap();

        let result = component
            .execute(&mock.run_context(), None, input_value.into())
            .await
            .unwrap();

        match result {
            FlowResult::Success(result) => {
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
        let mock = MockContext::new().await;

        // Create blob
        let create_input = PutBlobInput {
            data: test_data.clone(),
            blob_type: stepflow_core::BlobType::Data,
        };
        let create_input_value = serde_json::to_value(create_input).unwrap();

        let create_result = create_component
            .execute(&mock.run_context(), None, create_input_value.into())
            .await
            .unwrap();

        let blob_id = match create_result {
            FlowResult::Success(result) => {
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
            .execute(&mock.run_context(), None, get_input_value.into())
            .await
            .unwrap();

        match get_result {
            FlowResult::Success(result) => {
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
        let mock = MockContext::new().await;

        let result = component
            .execute(&mock.run_context(), None, input_value.into())
            .await;

        // Should return an error for invalid blob ID
        assert!(result.is_err());
    }
}
