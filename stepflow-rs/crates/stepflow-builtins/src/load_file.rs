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
use std::path::Path;
use stepflow_core::workflow::Component;
use stepflow_core::{FlowResult, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;
use tokio::fs;

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for loading data from files (JSON, YAML, or plain text)
pub struct LoadFileComponent;

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
struct LoadFileInput {
    /// Path to the file to load
    path: String,

    /// Format of the file (json, yaml, text). If not specified, inferred from extension
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<FileFormat>,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
enum FileFormat {
    Json,
    Yaml,
    Text,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
struct LoadFileOutput {
    /// The loaded data (parsed JSON/YAML or raw text)
    data: serde_json::Value,

    /// Metadata about the loaded file
    metadata: FileMetadata,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
struct FileMetadata {
    /// The resolved absolute path
    resolved_path: String,

    /// File size in bytes
    size_bytes: u64,

    /// Detected or specified format
    format: FileFormat,
}

impl LoadFileComponent {
    fn detect_format(path: &Path) -> FileFormat {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => FileFormat::Json,
            Some("yml") | Some("yaml") => FileFormat::Yaml,
            _ => FileFormat::Text,
        }
    }

    async fn load_file_content(path: &Path, format: FileFormat) -> Result<serde_json::Value> {
        let content = fs::read_to_string(path)
            .await
            .change_context(BuiltinError::InvalidInput)?;

        match format {
            FileFormat::Json => {
                serde_json::from_str(&content).change_context(BuiltinError::InvalidInput)
            }
            FileFormat::Yaml => {
                serde_yaml_ng::from_str(&content).change_context(BuiltinError::InvalidInput)
            }
            FileFormat::Text => Ok(serde_json::Value::String(content)),
        }
    }
}

impl BuiltinComponent for LoadFileComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<LoadFileInput>();
        let output_schema = SchemaRef::for_type::<LoadFileOutput>();

        Ok(ComponentInfo {
            component: Component::from_string("/load_file"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Load and parse a file (JSON, YAML, or text) from the filesystem".to_string(),
            ),
        })
    }

    async fn execute(&self, context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let LoadFileInput { path, format } = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        // Resolve the file path using the execution context's working directory
        let base_dir = context.working_directory();

        let file_path = if Path::new(&path).is_absolute() {
            Path::new(&path).to_path_buf()
        } else {
            base_dir.join(&path)
        };

        // Check if file exists
        if !file_path.exists() {
            return Err(BuiltinError::InvalidInput.into());
        }

        // Detect format if not specified
        let format = format.unwrap_or_else(|| Self::detect_format(&file_path));

        // Get file metadata
        let metadata_result = fs::metadata(&file_path)
            .await
            .change_context(BuiltinError::InvalidInput)?;

        // Load and parse the file
        let data = Self::load_file_content(&file_path, format).await?;

        let output = LoadFileOutput {
            data,
            metadata: FileMetadata {
                resolved_path: file_path.to_string_lossy().to_string(),
                size_bytes: metadata_result.len(),
                format,
            },
        };

        let output_value = serde_json::to_value(output).change_context(BuiltinError::Internal)?;

        Ok(FlowResult::Success(ValueRef::new(output_value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;
    use std::fs;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_json_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let test_data = r#"{"name": "test", "value": 42}"#;
        fs::write(temp_file.path(), test_data).unwrap();

        let component = LoadFileComponent;
        let input = LoadFileInput {
            path: temp_file.path().to_string_lossy().to_string(),
            format: Some(FileFormat::Json),
        };

        let input_value = serde_json::to_value(input).unwrap();
        let mock = MockContext::new().await;
        let result = component
            .execute(mock.execution_context(), input_value.into())
            .await
            .unwrap();

        let output: LoadFileOutput = result.success().unwrap().deserialize().unwrap();
        assert_eq!(output.data["name"], "test");
        assert_eq!(output.data["value"], 42);
        assert_eq!(output.metadata.format, FileFormat::Json);
    }

    #[tokio::test]
    async fn test_format_detection() {
        assert!(matches!(
            LoadFileComponent::detect_format(Path::new("test.json")),
            FileFormat::Json
        ));
        assert!(matches!(
            LoadFileComponent::detect_format(Path::new("test.yaml")),
            FileFormat::Yaml
        ));
        assert!(matches!(
            LoadFileComponent::detect_format(Path::new("test.txt")),
            FileFormat::Text
        ));
    }
}
