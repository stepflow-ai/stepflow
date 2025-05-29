use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use stepflow_core::{FlowResult, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;
use tokio::fs;

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// Component for loading data from files (JSON, YAML, or plain text)
pub struct LoadFileComponent;

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct LoadFileInput {
    /// Path to the file to load
    path: String,

    /// Format of the file (json, yaml, text). If not specified, inferred from extension
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<FileFormat>,

    /// Working directory to resolve relative paths (defaults to current directory)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    working_directory: Option<String>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
enum FileFormat {
    Json,
    Yaml,
    Text,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct LoadFileOutput {
    /// The loaded data (parsed JSON/YAML or raw text)
    data: serde_json::Value,

    /// Metadata about the loaded file
    metadata: FileMetadata,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
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
                serde_yml::from_str(&content).change_context(BuiltinError::InvalidInput)
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
            input_schema,
            output_schema,
        })
    }

    async fn execute(
        &self,
        _context: Arc<dyn ExecutionContext>,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let LoadFileInput {
            path,
            format,
            working_directory,
        } = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        // Resolve the file path
        let base_dir = working_directory
            .as_deref()
            .map(Path::new)
            .unwrap_or_else(|| Path::new("."));

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

        Ok(FlowResult::Success {
            result: ValueRef::new(output_value),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use stepflow_plugin::ExecutionContext;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_json_file() {
        // Mock context for testing
        struct MockContext;
        impl ExecutionContext for MockContext {
            fn execution_id(&self) -> uuid::Uuid {
                uuid::Uuid::new_v4()
            }
            fn submit_flow(
                &self,
                _flow: std::sync::Arc<stepflow_core::workflow::Flow>,
                _input: stepflow_core::workflow::ValueRef,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = stepflow_plugin::Result<uuid::Uuid>>
                        + Send
                        + '_,
                >,
            > {
                Box::pin(async { Ok(uuid::Uuid::new_v4()) })
            }
            fn flow_result(
                &self,
                _execution_id: uuid::Uuid,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = stepflow_plugin::Result<stepflow_core::FlowResult>,
                        > + Send
                        + '_,
                >,
            > {
                Box::pin(async { Ok(stepflow_core::FlowResult::Skipped) })
            }
        }

        let temp_file = NamedTempFile::new().unwrap();
        let test_data = r#"{"name": "test", "value": 42}"#;
        fs::write(temp_file.path(), test_data).unwrap();

        let component = LoadFileComponent;
        let input = LoadFileInput {
            path: temp_file.path().to_string_lossy().to_string(),
            format: Some(FileFormat::Json),
            working_directory: None,
        };

        let input_value = serde_json::to_value(input).unwrap();
        let context = std::sync::Arc::new(MockContext) as std::sync::Arc<dyn ExecutionContext>;
        let result = component
            .execute(context, input_value.into())
            .await
            .unwrap();

        let output: LoadFileOutput =
            serde_json::from_value(result.success().unwrap().clone()).unwrap();
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
