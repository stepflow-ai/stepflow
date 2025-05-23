use super::{Expr, Step};
use indexmap::IndexMap;
use schemars::JsonSchema;
use crate::schema::ObjectSchema;

/// A workflow consisting of a sequence of steps and their outputs.
///
/// A flow represents a complete workflow that can be executed. It contains:
/// - A sequence of steps to execute
/// - Named outputs that can reference step outputs
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Default, JsonSchema)]
pub struct Flow {
    /// The inputs of the flow, mapping input names to their JSON schemas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<ObjectSchema>,

    /// The output schema of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<ObjectSchema>,

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
    use crate::workflow::{Component, Expr};
    use indexmap::indexmap;

    use super::*;

    #[test]
    fn test_flow_from_yaml() {
        let yaml = r#"
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
              a: { literal: "hello world" }
          - component: mcp+http://foo/bar
            id: s2
            args:
              a: { literal: "hello world 2" }
        outputs:
            s1a: { step: "s1", field: "a" }
            s2b: { step: "s2", field: "a" }
        output_schema:
            type: object
            properties:
                s1a:
                    type: string
                s2b:
                    type: string
        "#;
        let flow: Flow = serde_yml::from_str(yaml).unwrap();
        let input_schema = ObjectSchema::parse(r#"{"type":"object","properties":{"name":{"type":"string","description":"The name to echo"},"count":{"type":"integer"}}}"#).unwrap();
        let output_schema = ObjectSchema::parse(
            r#"{"type":"object","properties":{"s1a":{"type":"string"},"s2b":{"type":"string"}}}"#,
        )
        .unwrap();
        similar_asserts::assert_serde_eq!(
            flow,
            Flow {
                input_schema: Some(input_schema),
                steps: vec![
                    Step {
                        id: "s1".to_owned(),
                        component: Component::parse("langflow://echo").unwrap(),
                        args: indexmap! {
                            "a".to_owned() => Expr::literal("hello world"),
                        },
                    },
                    Step {
                        id: "s2".to_owned(),
                        component: Component::parse("mcp+http://foo/bar").unwrap(),
                        args: indexmap! {
                            "a".to_owned() => Expr::literal("hello world 2"),
                        },
                    }
                ],
                outputs: indexmap! {
                    "s1a".to_owned() => Expr::step_field("s1", "a"),
                    "s2b".to_owned() => Expr::step_field("s2", "a"),
                },
                output_schema: Some(output_schema),
            }
        );
    }
}
