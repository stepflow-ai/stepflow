use indexmap::IndexMap;

use crate::{step::Step, Expr};

/// A workflow consisting of a sequence of steps and their outputs.
///
/// A flow represents a complete workflow that can be executed. It contains:
/// - A sequence of steps to execute
/// - Named outputs that can reference step outputs
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Flow {
    /// The steps to execute for the flow.
    pub steps: Vec<Step>,

    /// The outputs of the flow, mapping output names to their values.
    #[serde(default, with = "outputs_serde")]
    pub outputs: IndexMap<String, Expr>,
}

mod outputs_serde {
    use indexmap::IndexMap;
    use serde::de::Visitor;
    use serde::{Deserializer, Serializer};

    use crate::Expr;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Output {
        name: String,
        value: Expr,
    }

    pub fn serialize<S>(outputs: &IndexMap<String, Expr>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(outputs.iter().map(|(k, v)| Output {
            name: k.clone(),
            value: v.clone(),
        }))
    }

    struct OutputMapVisitor;

    impl<'de> Visitor<'de> for OutputMapVisitor {
        // The type that our Visitor is going to produce.
        type Value = IndexMap<String, Expr>;

        // Format a message stating what data this Visitor expects to receive.
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("output map")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut map = IndexMap::with_capacity(seq.size_hint().unwrap_or(1));
            while let Some(Output { name, value }) = seq.next_element()? {
                map.insert(name, value);
            }
            Ok(map)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<IndexMap<String, Expr>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(OutputMapVisitor)
    }
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
    use crate::{Component, Expr};
    use indexmap::indexmap;

    use super::*;

    #[test]
    fn test_flow_from_yaml() {
        let yaml = r#"
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
            - name: s1a
              value: { step: "s1", output: "a" }
            - name: s2b
              value: { step: "s2", output: "a" }
        "#;
        let flow: Flow = serde_yml::from_str(yaml).unwrap();
        assert_eq!(
            flow,
            Flow {
                steps: vec![
                    Step {
                        id: Some("s1".to_owned()),
                        component: Component::parse("langflow://echo").unwrap(),
                        args: indexmap! {
                            "a".to_owned() => Expr::literal("hello world"),
                        },
                        execution: None,
                    },
                    Step {
                        id: Some("s2".to_owned()),
                        component: Component::parse("mcp+http://foo/bar").unwrap(),
                        args: indexmap! {
                            "a".to_owned() => Expr::literal("hello world 2"),
                        },
                        execution: None,
                    }
                ],
                outputs: indexmap! {
                    "s1a".to_owned() => Expr::step("s1", "a"),
                    "s2b".to_owned() => Expr::step("s2", "a"),
                },
            }
        );
    }
}
