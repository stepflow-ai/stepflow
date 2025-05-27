use super::{Expr, Step};
use crate::schema::SchemaRef;
use indexmap::IndexMap;
use schemars::JsonSchema;

/// A workflow consisting of a sequence of steps and their outputs.
///
/// A flow represents a complete workflow that can be executed. It contains:
/// - A sequence of steps to execute
/// - Named outputs that can reference step outputs
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Default, JsonSchema)]
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
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub outputs: IndexMap<String, Expr>,
}

impl Flow {
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

    /// Returns a reference to the flow's outputs.
    pub fn outputs(&self) -> &IndexMap<String, Expr> {
        &self.outputs
    }

    /// Parses a flow from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is invalid or cannot be deserialized into a Flow.
    pub fn from_yaml_string(yaml: &str) -> serde_yml::Result<Self> {
        serde_yml::from_str(yaml)
    }

    /// Parses a flow from a YAML reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is invalid or cannot be deserialized into a Flow.
    pub fn from_yaml_reader(rdr: impl std::io::Read) -> serde_yml::Result<Self> {
        serde_yml::from_reader(rdr)
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
    pub fn to_yaml_string(&self) -> serde_yml::Result<String> {
        serde_yml::to_string(self)
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

#[cfg(test)]
mod tests {
    use crate::workflow::{Component, ErrorAction, Expr, SkipAction};
    use indexmap::indexmap;

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
            args:
              a: "hello world"
          - component: mcp+http://foo/bar
            id: s2
            args:
              a: "hello world 2"
        outputs:
            s1a: { $from: "s1", path: "a" }
            s2b: { $from: s2, path: a }
        output_schema:
            type: object
            properties:
                s1a:
                    type: string
                s2b:
                    type: string
        "#;
        let flow: Flow = serde_yml::from_str(yaml).unwrap();
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
                        args: indexmap! {
                            "a".to_owned() => Expr::literal("hello world"),
                        },
                        input_schema: None,
                        output_schema: None,
                        skip_if: None,
                        on_error: ErrorAction::default(),
                    },
                    Step {
                        id: "s2".to_owned(),
                        component: Component::parse("mcp+http://foo/bar").unwrap(),
                        args: indexmap! {
                            "a".to_owned() => Expr::literal("hello world 2"),
                        },
                        input_schema: None,
                        output_schema: None,
                        skip_if: None,
                        on_error: ErrorAction::default(),
                    }
                ],
                outputs: indexmap! {
                    "s1a".to_owned() => Expr::step_path("s1", "a", SkipAction::default()),
                    "s2b".to_owned() => Expr::step_path("s2", "a", SkipAction::default()),
                },
                output_schema: Some(output_schema),
            }
        );
    }
}
