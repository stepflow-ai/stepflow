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

use std::collections::HashMap;

use indexmap::IndexMap;
use serde_with::{DefaultOnNull, serde_as};

use super::{Step, ValueRef, VariableSchema};
use crate::{FlowResult, ValueExpr, schema::SchemaRef};

/// Consolidated schema information for a flow.
///
/// This struct contains all schema/type information for the flow in a single location,
/// allowing shared `$defs` across all schemas and avoiding duplication.
///
/// Serializes as a valid JSON Schema with `type: "object"` and flow-specific
/// properties (`input`, `output`, `variables`, `steps`) under the `properties` key.
#[derive(Debug, Clone, PartialEq, Default, utoipa::ToSchema)]
#[schema(default)]
pub struct FlowSchema {
    /// Shared type definitions that can be referenced by other schemas.
    /// References use the format `#/schemas/$defs/TypeName`.
    #[schema(default)]
    pub defs: HashMap<String, SchemaRef>,

    /// The input schema for the flow.
    pub input: Option<SchemaRef>,

    /// The output schema for the flow.
    pub output: Option<SchemaRef>,

    /// Schema for workflow variables. This is a JSON Schema object where
    /// properties define the available variables and their types.
    pub variables: Option<SchemaRef>,

    /// Output schemas for each step, keyed by step ID.
    /// Note: Step input schemas are not included here as they are
    /// component metadata, not flow-specific schemas.
    /// Uses IndexMap to preserve insertion order for deterministic serialization.
    #[schema(default)]
    pub steps: IndexMap<String, SchemaRef>,
}

impl serde::Serialize for FlowSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        // Build properties object
        let mut properties = serde_json::Map::new();

        if let Some(input) = &self.input {
            properties.insert(
                "input".to_string(),
                serde_json::to_value(input).map_err(serde::ser::Error::custom)?,
            );
        }

        if let Some(output) = &self.output {
            properties.insert(
                "output".to_string(),
                serde_json::to_value(output).map_err(serde::ser::Error::custom)?,
            );
        }

        if let Some(variables) = &self.variables {
            properties.insert(
                "variables".to_string(),
                serde_json::to_value(variables).map_err(serde::ser::Error::custom)?,
            );
        }

        if !self.steps.is_empty() {
            // Build steps as { type: object, properties: { step1: ..., step2: ... } }
            let mut step_properties = serde_json::Map::new();
            for (step_id, step_schema) in &self.steps {
                step_properties.insert(
                    step_id.clone(),
                    serde_json::to_value(step_schema).map_err(serde::ser::Error::custom)?,
                );
            }

            let steps_schema = serde_json::json!({
                "type": "object",
                "properties": step_properties
            });
            properties.insert("steps".to_string(), steps_schema);
        }

        // Count fields to serialize
        let mut field_count = 1; // type is always present
        if !self.defs.is_empty() {
            field_count += 1;
        }
        if !properties.is_empty() {
            field_count += 1;
        }

        let mut map = serializer.serialize_map(Some(field_count))?;

        map.serialize_entry("type", "object")?;

        if !self.defs.is_empty() {
            map.serialize_entry("$defs", &self.defs)?;
        }

        if !properties.is_empty() {
            map.serialize_entry("properties", &properties)?;
        }

        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for FlowSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // Deserialize as a generic JSON value first
        let value = serde_json::Value::deserialize(deserializer)?;

        // Handle null as empty schema
        if value.is_null() {
            return Ok(FlowSchema::default());
        }

        let obj = value
            .as_object()
            .ok_or_else(|| D::Error::custom("FlowSchema must be an object"))?;

        // Extract $defs
        let defs: HashMap<String, SchemaRef> = if let Some(defs_val) = obj.get("$defs") {
            if defs_val.is_null() {
                HashMap::new()
            } else {
                serde_json::from_value(defs_val.clone()).map_err(D::Error::custom)?
            }
        } else {
            HashMap::new()
        };

        // Extract properties
        let properties = obj.get("properties").and_then(|p| p.as_object());

        let input: Option<SchemaRef> = if let Some(props) = properties {
            props
                .get("input")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(D::Error::custom)?
        } else {
            None
        };

        let output: Option<SchemaRef> = if let Some(props) = properties {
            props
                .get("output")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(D::Error::custom)?
        } else {
            None
        };

        let variables: Option<SchemaRef> = if let Some(props) = properties {
            props
                .get("variables")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(D::Error::custom)?
        } else {
            None
        };

        // Extract steps from properties.steps.properties
        let steps: IndexMap<String, SchemaRef> = if let Some(props) = properties {
            if let Some(steps_obj) = props.get("steps").and_then(|s| s.as_object()) {
                if let Some(step_properties) =
                    steps_obj.get("properties").and_then(|p| p.as_object())
                {
                    let mut steps_map = IndexMap::new();
                    for (step_id, step_schema) in step_properties {
                        let schema: SchemaRef =
                            serde_json::from_value(step_schema.clone()).map_err(D::Error::custom)?;
                        steps_map.insert(step_id.clone(), schema);
                    }
                    steps_map
                } else {
                    IndexMap::new()
                }
            } else {
                IndexMap::new()
            }
        } else {
            IndexMap::new()
        };

        Ok(FlowSchema {
            defs,
            input,
            output,
            variables,
            steps,
        })
    }
}

impl FlowSchema {
    /// Returns true if all fields are empty/None.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
            && self.input.is_none()
            && self.output.is_none()
            && self.variables.is_none()
            && self.steps.is_empty()
    }
}

/// A workflow consisting of a sequence of steps and their outputs.
///
/// A flow represents a complete workflow that can be executed. It contains:
/// - A sequence of steps to execute
/// - Named outputs that can reference step outputs
///
/// Flows should not be cloned. They should generally be stored and passed as a
/// reference or inside an `Arc`.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
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

    /// Create a clone of this flow.
    ///
    /// **Warning**: This method performs a deep clone of the entire workflow structure,
    /// including all steps, metadata, and configurations. This can be expensive for
    /// large workflows.
    ///
    /// # Performance
    /// - Cloning large workflows with many steps can be slow
    /// - Consider using `Arc<Flow>` for shared ownership instead
    /// - Only use this when you need to modify the workflow structure
    ///
    /// # Example
    /// ```rust
    /// use stepflow_core::workflow::Flow;
    ///
    /// let original_flow = Flow::default();
    /// let cloned_flow = original_flow.slow_clone();
    /// ```
    pub fn slow_clone(&self) -> Self {
        match self {
            Flow::V1(flow_v1) => Flow::V1(flow_v1.clone()),
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

    /// Get the variable schema for the flow.
    ///
    /// This constructs a `VariableSchema` from the schema definition, extracting
    /// runtime metadata like defaults, secrets, and required variables.
    pub fn variables(&self) -> Option<VariableSchema> {
        self.schemas().variables.clone().map(VariableSchema::from)
    }

    /// Get a reference to the variable schema (raw SchemaRef).
    pub fn variable_schema(&self) -> Option<&SchemaRef> {
        self.schemas().variables.as_ref()
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
    pub fn output(&self) -> &ValueExpr {
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

    /// Get the flow's schema information.
    pub fn schemas(&self) -> &FlowSchema {
        &self.latest().schemas
    }

    /// Get a mutable reference to the flow's schema information.
    pub fn schemas_mut(&mut self) -> &mut FlowSchema {
        match self {
            Flow::V1(flow_v1) => &mut flow_v1.schemas,
        }
    }

    /// Get the flow's input schema.
    pub fn input_schema(&self) -> Option<&SchemaRef> {
        self.latest().schemas.input.as_ref()
    }

    /// Set the flow's input schema.
    pub fn set_input_schema(&mut self, input_schema: Option<SchemaRef>) {
        match self {
            Flow::V1(flow_v1) => flow_v1.schemas.input = input_schema,
        }
    }

    /// Get the flow's output schema.
    pub fn output_schema(&self) -> Option<&SchemaRef> {
        self.latest().schemas.output.as_ref()
    }

    /// Set the flow's output schema.
    pub fn set_output_schema(&mut self, output_schema: Option<SchemaRef>) {
        match self {
            Flow::V1(flow_v1) => flow_v1.schemas.output = output_schema,
        }
    }

    /// Get the output schema for a specific step.
    pub fn step_output_schema(&self, step_id: &str) -> Option<&SchemaRef> {
        self.latest().schemas.steps.get(step_id)
    }

    /// Set the output schema for a specific step.
    pub fn set_step_output_schema(&mut self, step_id: String, step_schema: SchemaRef) {
        match self {
            Flow::V1(flow_v1) => {
                flow_v1.schemas.steps.insert(step_id, step_schema);
            }
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
#[serde_as]
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Default, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
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

    /// Consolidated schema information for the flow.
    /// Contains input/output schemas, step output schemas, and shared `$defs`.
    #[serde(default, skip_serializing_if = "FlowSchema::is_empty")]
    #[serde_as(as = "DefaultOnNull")]
    pub schemas: FlowSchema,

    /// The steps to execute for the flow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,

    /// The outputs of the flow, mapping output names to their values.
    #[serde(default, skip_serializing_if = "ValueExpr::is_null")]
    pub output: ValueExpr,

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

/// Configuration for a test server.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
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
    #[schema(default = default_startup_timeout)]
    pub startup_timeout_ms: u64,

    /// Maximum time to wait for server shutdown (in milliseconds).
    #[serde(default = "default_shutdown_timeout")]
    #[schema(default = default_shutdown_timeout)]
    pub shutdown_timeout_ms: u64,
}

/// Health check configuration for test servers.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestServerHealthCheck {
    /// Path for health check endpoint (e.g., "/health").
    pub path: String,

    /// Timeout for health check requests (in milliseconds).
    #[serde(default = "default_health_check_timeout")]
    #[schema(default = default_health_check_timeout)]
    pub timeout_ms: u64,

    /// Number of retry attempts for health checks.
    #[serde(default = "default_health_check_retries")]
    #[schema(default = default_health_check_retries)]
    pub retry_attempts: u32,

    /// Delay between retry attempts (in milliseconds).
    #[serde(default = "default_health_check_delay")]
    #[schema(default = default_health_check_delay)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
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
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, utoipa::ToSchema)]
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
        schemas:
            type: object
            properties:
                input:
                    type: object
                    properties:
                        name:
                            type: string
                            description: The name to echo
                        count:
                            type: integer
                output:
                    type: object
                    properties:
                        s1a:
                            type: string
                        s2b:
                            type: string
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
            s1a: { $step: s1, path: "a" }
            s2b: { $step: s2, path: a }
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
        assert_eq!(latest.schemas.input, Some(input_schema.clone()));
        assert_eq!(latest.schemas.output, Some(output_schema.clone()));
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
        // The output should be parsed as an Object expression containing step references
        assert!(matches!(latest.output, ValueExpr::Object(_)));

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
                serde_json::from_value(serde_json::json!({
                    "s1a": { "$step": "s1", "path": "a" },
                    "s2b": { "$step": "s2", "path": "a" }
                }))
                .unwrap(),
            )
            .build();

        let Flow::V1(expected_flow) = expected_flow_built;

        similar_asserts::assert_serde_eq!(latest, &expected_flow);
    }

    #[test]
    fn test_get_all_examples() {
        use super::*;
        use serde_json::json;

        // Create a flow with both examples and test cases
        let flow = FlowBuilder::new()
            .name("test_flow")
            .output(ValueExpr::literal(json!({})))
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
        use crate::json_schema::generate_json_schema_with_defs;
        use std::env;

        // Generate schema from Rust types using utoipa-based utility
        let generated_json = generate_json_schema_with_defs::<Flow>();
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
            eprintln!("Updated flow.json at {}", flow_schema_path);
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

    #[test]
    fn test_utoipa_schema_generation() {
        use crate::json_schema::generate_json_schema_with_defs;

        // Generate schema using the utoipa-based utility
        let generated = generate_json_schema_with_defs::<Flow>();
        let generated_str = serde_json::to_string_pretty(&generated).unwrap();

        // Print the schema for manual comparison during development
        // This test is primarily for development/debugging - will be replaced
        // by the actual schema comparison test once we verify the output
        if std::env::var("PRINT_SCHEMA").is_ok() {
            eprintln!("Generated Flow schema:\n{}", generated_str);
        }

        // Basic structural checks
        assert!(generated.get("$schema").is_some(), "Missing $schema");
        assert!(generated.get("title").is_some(), "Missing title");

        // Should have $defs with referenced types
        let defs = generated.get("$defs");
        assert!(defs.is_some(), "Missing $defs");

        // Verify no #/components/schemas references remain
        assert!(
            !generated_str.contains("#/components/schemas"),
            "Found unconverted #/components/schemas reference"
        );
    }
}
