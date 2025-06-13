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
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub output: serde_json::Value,

    /// Test configuration for the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestConfig>,
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
pub struct FlowHash(String);

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
    pub fn output(&self) -> &serde_json::Value {
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
    fn schema_name() -> String {
        Flow::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::schema::Schema {
        Flow::json_schema(generator)
    }

    fn is_referenceable() -> bool {
        Flow::is_referenceable()
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
        input_schema:
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
        output_schema:
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
                }),
                output_schema: Some(output_schema),
                test: None,
            }
        );
    }
}

/// Configuration for testing a workflow.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema, utoipa::ToSchema)]
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
