// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::borrow::Cow;

use super::{Step, ValueRef};
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
#[derive(
    Debug, serde::Serialize, serde::Deserialize, PartialEq, Default, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,

    /// The outputs of the flow, mapping output names to their values.
    #[serde(default, skip_serializing_if = "ValueRef::is_null")]
    pub output: ValueRef,

    /// Test configuration for the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestConfig>,

    /// Example inputs for the workflow that can be used for testing and UI dropdowns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ExampleInput>,
}

/// Wraper around a string that represents a hash of a workflow.
#[derive(
    Eq,
    Debug,
    PartialEq,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
    schemars::JsonSchema,
)]
#[repr(transparent)]
pub struct FlowHash(#[schema(inline)] String);

impl From<&str> for FlowHash {
    fn from(hash: &str) -> Self {
        FlowHash(hash.to_owned())
    }
}

impl std::fmt::Display for FlowHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Flow {
    /// Return a hash of the workflow.
    pub fn hash(workflow: &Self) -> FlowHash {
        use sha2::{Digest as _, Sha256};
        let mut hasher = Sha256::new();
        serde_json::to_writer(&mut hasher, workflow).unwrap();
        FlowHash(format!("{:x}", hasher.finalize()))
    }

    /// Returns a reference to all steps in the flow.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Returns a reference to the step at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn step(&self, index: usize) -> &Step {
        &self.steps[index]
    }

    /// Returns a reference to the flow's output value.
    pub fn output(&self) -> &ValueRef {
        &self.output
    }

    /// Parses a flow from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is invalid or cannot be deserialized into a Flow.
    pub fn from_yaml_string(yaml: &str) -> serde_yaml_ng::Result<Self> {
        serde_yaml_ng::from_str(yaml)
    }

    /// Parses a flow from a YAML reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is invalid or cannot be deserialized into a Flow.
    pub fn from_yaml_reader(rdr: impl std::io::Read) -> serde_yaml_ng::Result<Self> {
        serde_yaml_ng::from_reader(rdr)
    }

    /// Parses a flow from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid or cannot be deserialized into a Flow.
    pub fn from_json_string(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Serializes the flow to a YAML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow cannot be serialized to YAML.
    pub fn to_yaml_string(&self) -> serde_yaml_ng::Result<String> {
        serde_yaml_ng::to_string(self)
    }

    /// Serializes the flow to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow cannot be serialized to JSON.
    pub fn to_json_string(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

/// A wrapper around Arc<Flow> to support poem-openapi traits.
///
/// This wrapper exists to work around Rust's orphan rules which prevent
/// implementing external traits on external types like Arc<Flow>.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowRef(std::sync::Arc<Flow>);

impl FlowRef {
    /// Create a new FlowRef from a Flow.
    pub fn new(flow: Flow) -> Self {
        Self(std::sync::Arc::new(flow))
    }

    /// Create a new FlowRef from an Arc<Flow>.
    pub fn from_arc(arc: std::sync::Arc<Flow>) -> Self {
        Self(arc)
    }

    /// Get a reference to the underlying Flow.
    pub fn as_flow(&self) -> &Flow {
        &self.0
    }

    /// Get the underlying Arc<Flow>.
    pub fn into_arc(self) -> std::sync::Arc<Flow> {
        self.0
    }

    /// Get a reference to the underlying Arc<Flow>.
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

#[cfg(test)]
mod tests {
    use crate::workflow::{Component, ErrorAction};

    use super::*;

    #[test]
    fn test_flow_from_yaml() {
        let yaml = r#"
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
          - component: langflow://echo
            id: s1
            input:
              a: "hello world"
          - component: mcp+http://foo/bar
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
        similar_asserts::assert_serde_eq!(
            flow,
            Flow {
                name: Some("test".to_owned()),
                description: Some("test".to_owned()),
                version: Some("1.0.0".to_owned()),
                input_schema: Some(input_schema),
                steps: vec![
                    Step {
                        id: "s1".to_owned(),
                        component: Component::parse("langflow://echo").unwrap(),
                        input: serde_json::json!({
                            "a": "hello world"
                        })
                        .into(),
                        input_schema: None,
                        output_schema: None,
                        skip_if: None,
                        on_error: ErrorAction::default(),
                    },
                    Step {
                        id: "s2".to_owned(),
                        component: Component::parse("mcp+http://foo/bar").unwrap(),
                        input: serde_json::json!({
                            "a": "hello world 2"
                        })
                        .into(),
                        input_schema: None,
                        output_schema: None,
                        skip_if: None,
                        on_error: ErrorAction::default(),
                    }
                ],
                output: serde_json::json!({
                    "s1a": { "$from": { "step": "s1" }, "path": "a" },
                    "s2b": { "$from": { "step": "s2" }, "path": "a" }
                })
                .into(),
                output_schema: Some(output_schema),
                test: None,
                examples: vec![],
            }
        );
    }

    #[test]
    fn test_get_all_examples() {
        use super::*;
        use serde_json::json;

        // Create a flow with both examples and test cases
        let flow = Flow {
            name: Some("test_flow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![],
            output: json!({}).into(),
            examples: vec![ExampleInput {
                name: "example1".to_string(),
                description: Some("Direct example".to_string()),
                input: ValueRef::new(json!({"input": "example"})),
            }],
            test: Some(TestConfig {
                stepflow_config: None,
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
            }),
        };

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
}

/// Configuration for testing a workflow.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestConfig {
    /// Stepflow configuration specific to tests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepflow_config: Option<serde_json::Value>,

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

impl Flow {
    /// Get all example inputs, including those derived from test cases.
    pub fn get_all_examples(&self) -> Vec<ExampleInput> {
        let mut examples = self.examples.clone();

        // Add examples from test cases if they exist
        if let Some(test_config) = &self.test {
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

#[cfg(test)]
mod schema_tests {
    use super::*;
    use schemars::schema_for;
    use std::env;

    #[test]
    fn test_schema_comparison_with_flow_json() {
        let generated_schema = schema_for!(Flow);
        let generated_json = serde_json::to_value(&generated_schema)
            .expect("Failed to convert generated schema to JSON");

        let flow_schema_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../schemas/flow.json"
        );

        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            // Create directory and write schema
            if let Some(parent) = std::path::Path::new(flow_schema_path).parent() {
                std::fs::create_dir_all(parent).expect("Failed to create schema directory");
            }
            std::fs::write(
                flow_schema_path,
                serde_json::to_string_pretty(&generated_json).unwrap(),
            )
            .expect("Failed to write updated schema");
        } else {
            // Compare with existing schema
            match std::fs::read_to_string(flow_schema_path) {
                Ok(flow_schema_str) => {
                    let flow_schema: serde_json::Value = 
                        serde_json::from_str(&flow_schema_str)
                            .expect("Failed to parse flow schema JSON");
                    
                    let generated_schema_str = serde_json::to_string_pretty(&generated_json)
                        .expect("Failed to serialize generated schema");
                    let expected_schema_str = serde_json::to_string_pretty(&flow_schema)
                        .expect("Failed to serialize expected schema");

                    similar_asserts::assert_eq!(generated_schema_str, expected_schema_str, 
                        "Generated schema does not match the reference schema at {}. \
                         Run with STEPFLOW_OVERWRITE_SCHEMA=1 to update the reference schema.",
                        flow_schema_path);
                }
                Err(_) => {
                    panic!("Flow schema file not found at {flow_schema_path}. \
                           Run with STEPFLOW_OVERWRITE_SCHEMA=1 to create it.");
                }
            }
        }
    }
}
