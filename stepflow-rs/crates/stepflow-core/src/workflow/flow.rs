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

use std::borrow::Cow;
use std::collections::HashMap;

use super::{Step, ValueRef, ValueTemplate};
use crate::{FlowResult, schema::SchemaRef};
use schemars::JsonSchema;

/// A workflow consisting of a sequence of steps and their outputs.
///
/// A flow represents a complete workflow that can be executed. It contains:
/// - A sequence of steps to execute
/// - Named outputs that can reference step outputs
///
/// Flows should not be cloned. They should generally be stored and passed as a
/// reference or inside an `Arc`.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "schema")]
pub enum Flow {
    #[serde(rename = "https://stepflow.org/schemas/v1/flow.json")]
    V1(FlowV1),
}

impl Default for Flow {
    fn default() -> Self {
        Flow::V1(FlowV1::default())
    }
}

impl Flow {
    pub fn supported_schema(schema: &str) -> bool {
        schema == "https://stepflow.org/schemas/v1/flow.json"
    }

    /// Upgrade the flow to the latest version.
    pub fn upgrade(self) -> Self {
        match self {
            Flow::V1(flow_v1) => Flow::V1(flow_v1),
        }
    }

    pub fn latest(&self) -> &FlowV1 {
        match self {
            Flow::V1(flow_v1) => flow_v1,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Flow::V1(flow_v1) => flow_v1.name.as_deref(),
        }
    }

    pub fn description(&self) -> Option<&str> {
        match self {
            Flow::V1(flow_v1) => flow_v1.description.as_deref(),
        }
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            Flow::V1(flow_v1) => flow_v1.version.as_deref(),
        }
    }

    pub fn metadata(&self) -> &HashMap<String, serde_json::Value> {
        match self {
            Flow::V1(flow_v1) => &flow_v1.metadata,
        }
    }

    /// Returns a reference to all steps in the flow.
    pub fn steps(&self) -> &[Step] {
        &self.latest().steps
    }

    pub fn examples(&self) -> &[ExampleInput] {
        match self {
            Flow::V1(flow_v1) => flow_v1.examples.as_deref().unwrap_or(&[]),
        }
    }

    /// Returns a reference to the step at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn step(&self, index: usize) -> &Step {
        &self.latest().steps[index]
    }

    /// Returns a mutable reference to the step at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn step_mut(&mut self, index: usize) -> &mut Step {
        match self {
            Flow::V1(flow_v1) => flow_v1.steps.get_mut(index).expect("Index out of bounds"),
        }
    }

    /// Returns a reference to the flow's output value.
    pub fn output(&self) -> &ValueTemplate {
        &self.latest().output
    }

    pub fn test(&self) -> Option<&TestConfig> {
        self.latest().test.as_ref()
    }

    pub fn test_mut(&mut self) -> Option<&mut TestConfig> {
        match self {
            Flow::V1(flow_v1) => flow_v1.test.as_mut(),
        }
    }

    /// Get all example inputs, including those derived from test cases.
    pub fn get_all_examples(&self) -> Vec<ExampleInput> {
        let mut examples = self.examples().to_vec();

        // Add examples from test cases if they exist
        if let Some(test_config) = &self.latest().test {
            for test_case in &test_config.cases {
                // Only add if there isn't already an example with the same name
                if !examples.iter().any(|ex| ex.name == test_case.name) {
                    examples.push(ExampleInput::from(test_case));
                }
            }
        }

        examples
    }
}

/// # FlowV1
#[derive(
    Debug, serde::Serialize, serde::Deserialize, PartialEq, Default, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct FlowV1 {
    /// The name of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The description of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The version of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// The input schema of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<SchemaRef>,

    /// The output schema of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<SchemaRef>,

    /// The steps to execute for the flow.
    #[schemars(extend("default" = []))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,

    /// The outputs of the flow, mapping output names to their values.
    #[serde(default, skip_serializing_if = "ValueTemplate::is_null")]
    pub output: ValueTemplate,

    /// Test configuration for the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestConfig>,

    /// Example inputs for the workflow that can be used for testing and UI dropdowns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<ExampleInput>>,

    /// Extensible metadata for the flow that can be used by tools and frameworks.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A wrapper around `Arc<Flow>` to support poem-openapi traits.
///
/// This wrapper exists to work around Rust's orphan rules which prevent
/// implementing external traits on external types like `Arc<Flow>`.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowRef(std::sync::Arc<Flow>);

impl FlowRef {
    /// Create a new FlowRef from a Flow.
    pub fn new(flow: Flow) -> Self {
        Self(std::sync::Arc::new(flow))
    }

    /// Create a new FlowRef from an `Arc<Flow>`.
    pub fn from_arc(arc: std::sync::Arc<Flow>) -> Self {
        Self(arc)
    }

    /// Get a reference to the underlying Flow.
    pub fn as_flow(&self) -> &Flow {
        &self.0
    }

    /// Get the underlying `Arc<Flow>`.
    pub fn into_arc(self) -> std::sync::Arc<Flow> {
        self.0
    }

    /// Get a reference to the underlying `Arc<Flow>`.
    pub fn as_arc(&self) -> &std::sync::Arc<Flow> {
        &self.0
    }
}

impl std::ops::Deref for FlowRef {
    type Target = Flow;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Flow> for FlowRef {
    fn from(flow: Flow) -> Self {
        Self::new(flow)
    }
}

impl From<std::sync::Arc<Flow>> for FlowRef {
    fn from(arc: std::sync::Arc<Flow>) -> Self {
        Self::from_arc(arc)
    }
}

impl serde::Serialize for FlowRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for FlowRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let flow = Flow::deserialize(deserializer)?;
        Ok(Self::new(flow))
    }
}

impl JsonSchema for FlowRef {
    fn schema_name() -> Cow<'static, str> {
        Flow::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        Flow::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        Flow::json_schema(generator)
    }

    fn inline_schema() -> bool {
        Flow::inline_schema()
    }
}
/// Configuration for a test server.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestServerConfig {
    /// Command to start the server.
    pub command: String,

    /// Arguments for the server command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Environment variables for the server process.
    /// Values can contain placeholders like {port} which will be substituted.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// Working directory for the server process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,

    /// Port range for automatic port allocation.
    /// If not specified, a random available port will be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_range: Option<(u16, u16)>,

    /// Health check configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<TestServerHealthCheck>,

    /// Maximum time to wait for server startup (in milliseconds).
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_ms: u64,

    /// Maximum time to wait for server shutdown (in milliseconds).
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_ms: u64,
}

/// Health check configuration for test servers.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestServerHealthCheck {
    /// Path for health check endpoint (e.g., "/health").
    pub path: String,

    /// Timeout for health check requests (in milliseconds).
    #[serde(default = "default_health_check_timeout")]
    pub timeout_ms: u64,

    /// Number of retry attempts for health checks.
    #[serde(default = "default_health_check_retries")]
    pub retry_attempts: u32,

    /// Delay between retry attempts (in milliseconds).
    #[serde(default = "default_health_check_delay")]
    pub retry_delay_ms: u64,
}

fn default_startup_timeout() -> u64 {
    10000
}
fn default_shutdown_timeout() -> u64 {
    5000
}
fn default_health_check_timeout() -> u64 {
    5000
}
fn default_health_check_retries() -> u32 {
    3
}
fn default_health_check_delay() -> u64 {
    1000
}

/// Configuration for testing a workflow.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestConfig {
    /// Test servers to start before running tests.
    /// Key is the server name, value is the server configuration.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub servers: HashMap<String, TestServerConfig>,

    /// Stepflow configuration specific to tests.
    /// Can reference server URLs using placeholders like {server_name.url}.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "stepflow_config"
    )]
    pub config: Option<serde_json::Value>,

    /// Test cases for the workflow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<TestCase>,
}

/// A single test case for a workflow.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestCase {
    /// Unique identifier for the test case.
    pub name: String,

    /// Optional description of what this test case verifies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Input data for the workflow in this test case.
    pub input: ValueRef,

    /// Expected output from the workflow for this test case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<FlowResult>,
}

/// An example input for a workflow that can be used in UI dropdowns.
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ExampleInput {
    /// Name of the example input for display purposes.
    pub name: String,

    /// Optional description of what this example demonstrates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The input data for this example.
    pub input: ValueRef,
}

impl From<&TestCase> for ExampleInput {
    fn from(test_case: &TestCase) -> Self {
        Self {
            name: test_case.name.clone(),
            description: test_case.description.clone(),
            input: test_case.input.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::workflow::{FlowBuilder, StepBuilder};

    use super::*;

    #[test]
    fn test_flow_from_yaml() {
        let yaml = r#"
        schema: https://stepflow.org/schemas/v1/flow.json
        name: test
        description: test
        version: 1.0.0
        inputSchema:
            type: object
            properties:
                name:
                    type: string
                    description: The name to echo
                count:
                    type: integer
        steps:
          - component: /langflow/echo
            id: s1
            input:
              a: "hello world"
          - component: /mcp/foo/bar
            id: s2
            input:
              a: "hello world 2"
        output:
            s1a: { $from: { step: s1 }, path: "a" }
            s2b: { $from: { step: s2 }, path: a }
        outputSchema:
            type: object
            properties:
                s1a:
                    type: string
                s2b:
                    type: string
        "#;
        let flow: Flow = serde_yaml_ng::from_str(yaml).unwrap();
        let input_schema = SchemaRef::parse_json(r#"{"type":"object","properties":{"name":{"type":"string","description":"The name to echo"},"count":{"type":"integer"}}}"#).unwrap();
        let output_schema = SchemaRef::parse_json(
            r#"{"type":"object","properties":{"s1a":{"type":"string"},"s2b":{"type":"string"}}}"#,
        )
        .unwrap();
        // Verify basic flow properties
        let latest = flow.latest();
        assert_eq!(latest.name, Some("test".to_owned()));
        assert_eq!(latest.description, Some("test".to_owned()));
        assert_eq!(latest.version, Some("1.0.0".to_owned()));
        assert_eq!(latest.input_schema, Some(input_schema.clone()));
        assert_eq!(latest.output_schema, Some(output_schema.clone()));
        assert_eq!(latest.steps.len(), 2);

        // Verify step details
        assert_eq!(latest.steps[0].id, "s1");
        assert_eq!(latest.steps[0].component.path(), "/langflow/echo");
        assert_eq!(latest.steps[1].id, "s2");
        assert_eq!(latest.steps[1].component.path(), "/mcp/foo/bar");

        // Test round-trip serialization to ensure expressions are preserved
        let serialized = serde_json::to_string(&flow).unwrap();
        let deserialized: Flow = serde_json::from_str(&serialized).unwrap();
        assert_eq!(latest.name, deserialized.latest().name);
        assert_eq!(latest.steps.len(), deserialized.latest().steps.len());
        assert_eq!(latest.output, deserialized.latest().output);

        // Verify that the output contains proper expression structures
        // The output should not be literal but should contain parsed expressions
        assert!(!latest.output.is_literal());

        // Test full structural equality
        let expected_flow_built = FlowBuilder::new()
            .name("test")
            .description("test")
            .version("1.0.0")
            .input_schema(input_schema)
            .output_schema(output_schema)
            .steps(vec![
                StepBuilder::new("s1")
                    .component("/langflow/echo")
                    .input_literal(serde_json::json!({
                        "a": "hello world"
                    }))
                    .build(),
                StepBuilder::new("s2")
                    .component("/mcp/foo/bar")
                    .input_literal(serde_json::json!({
                        "a": "hello world 2"
                    }))
                    .build(),
            ])
            .output(
                ValueTemplate::parse_value(serde_json::json!({
                    "s1a": { "$from": { "step": "s1" }, "path": "a" },
                    "s2b": { "$from": { "step": "s2" }, "path": "a" }
                }))
                .unwrap(),
            )
            .build();

        let expected_flow = match expected_flow_built {
            Flow::V1(flow_v1) => flow_v1,
        };

        similar_asserts::assert_serde_eq!(latest, &expected_flow);
    }

    #[test]
    fn test_get_all_examples() {
        use super::*;
        use serde_json::json;

        // Create a flow with both examples and test cases
        let flow = FlowBuilder::new()
            .name("test_flow")
            .output(ValueTemplate::literal(json!({})))
            .examples(vec![ExampleInput {
                name: "example1".to_string(),
                description: Some("Direct example".to_string()),
                input: ValueRef::new(json!({"input": "example"})),
            }])
            .test_config(TestConfig {
                config: None,
                servers: HashMap::default(),
                cases: vec![
                    TestCase {
                        name: "test1".to_string(),
                        description: Some("Test case as example".to_string()),
                        input: ValueRef::new(json!({"input": "test"})),
                        output: None,
                    },
                    TestCase {
                        name: "example1".to_string(), // Duplicate name, should not be added
                        description: Some("Duplicate name".to_string()),
                        input: ValueRef::new(json!({"input": "duplicate"})),
                        output: None,
                    },
                ],
            })
            .build();

        let all_examples = flow.get_all_examples();

        // Should have 2 examples: 1 direct + 1 from test cases (duplicate name ignored)
        assert_eq!(all_examples.len(), 2);

        // Check first example (direct)
        assert_eq!(all_examples[0].name, "example1");
        assert_eq!(
            all_examples[0].description,
            Some("Direct example".to_string())
        );

        // Check second example (from test case)
        assert_eq!(all_examples[1].name, "test1");
        assert_eq!(
            all_examples[1].description,
            Some("Test case as example".to_string())
        );
    }

    #[test]
    fn test_schema_comparison_with_flow_json() {
        use std::env;

        // Generate schema from Rust types
        let mut generator = schemars::generate::SchemaSettings::draft2020_12().into_generator();
        let generated_schema = generator.root_schema_for::<Flow>();
        let generated_json = serde_json::to_value(&generated_schema)
            .expect("Failed to convert generated schema to JSON");

        let generated_schema_str = serde_json::to_string_pretty(&generated_json).unwrap();

        let flow_schema_path = format!("{}/../../../schemas/flow.json", env!("CARGO_MANIFEST_DIR"));

        // Check if we should overwrite the reference schema or if it doesn't exist
        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            // Ensure the directory exists
            if let Some(parent) = std::path::Path::new(&flow_schema_path).parent() {
                std::fs::create_dir_all(parent).expect("Failed to create schema directory");
            }

            std::fs::write(&flow_schema_path, &generated_schema_str)
                .expect("Failed to write updated schema");
        } else {
            match std::fs::read_to_string(&flow_schema_path) {
                Ok(expected_schema_str) => {
                    // Use similar_asserts for better diff output when schemas don't match
                    assert_eq!(
                        generated_schema_str, expected_schema_str,
                        "Generated schema does not match the reference schema at {flow_schema_path}. \
                         Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-core' to update."
                    );
                }
                Err(_) => {
                    // File doesn't exist, fail the test with helpful message
                    panic!(
                        "Flow schema file not found at {flow_schema_path}. Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-core' to create it."
                    );
                }
            }
        }
    }
}
