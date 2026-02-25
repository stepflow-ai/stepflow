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

//! Flow schema types for consolidated schema information.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::schema::SchemaRef;

/// Consolidated schema information for a flow.
///
/// This struct contains all schema/type information for the flow in a single location,
/// allowing shared `$defs` across all schemas and avoiding duplication.
///
/// Serializes as a valid JSON Schema with `type: "object"` and flow-specific
/// properties (`input`, `output`, `variables`, `steps`) under the `properties` key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FlowSchema {
    /// Shared type definitions that can be referenced by other schemas.
    /// References use the format `#/schemas/$defs/TypeName`.
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
    pub steps: IndexMap<String, SchemaRef>,
}

impl schemars::JsonSchema for FlowSchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FlowSchema".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Describe FlowSchema as an opaque JSON Schema object.
        // The actual structure uses custom serde serialization that wraps fields
        // in JSON Schema format (type/properties/$defs), which doesn't match
        // the struct fields. By describing it as a free-form object, the OpenAPI
        // spec and generated SDKs treat it as dict[str, Any] and pass it through
        // without needing to understand the internal structure.
        schemars::json_schema!({
            "type": "object",
            "description": "A JSON Schema object describing the flow's type information. Contains input/output schemas, variable schemas, and step output schemas wrapped in standard JSON Schema format with type, properties, and $defs.",
            "additionalProperties": true
        })
    }
}

impl serde::Serialize for FlowSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap as _;

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
        use serde::de::Error as _;

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

        // FlowSchema serializes as a JSON Schema: {"type": "object", "properties": {...}}
        // Reject flat format (e.g. {"variables": ...}) — callers must send the correct
        // JSON Schema structure.
        let properties = obj.get("properties").and_then(|p| p.as_object());

        // Detect common mistake: flat format without "properties" wrapper
        if properties.is_none() {
            let flat_keys = ["input", "output", "variables", "steps"];
            for key in flat_keys {
                if obj.contains_key(key) {
                    return Err(D::Error::custom(format!(
                        "FlowSchema field \"{key}\" must be inside a \"properties\" wrapper. \
                         Expected JSON Schema format: \
                         {{\"type\": \"object\", \"properties\": {{\"{key}\": ...}}}}"
                    )));
                }
            }
        }

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
                        let schema: SchemaRef = serde_json::from_value(step_schema.clone())
                            .map_err(D::Error::custom)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::variable_schema::VariableSchema;
    use serde_json::json;

    #[test]
    fn test_deserialize_json_schema_format() {
        let json = json!({
            "type": "object",
            "properties": {
                "variables": {
                    "type": "object",
                    "properties": {
                        "API_KEY": {
                            "type": "string",
                            "env_var": "MY_API_KEY"
                        }
                    }
                }
            }
        });

        let schema: FlowSchema = serde_json::from_value(json).unwrap();
        assert!(schema.variables.is_some());

        let var_schema = VariableSchema::from(schema.variables.unwrap());
        assert_eq!(var_schema.env_var_name("API_KEY"), Some("MY_API_KEY"));
    }

    #[test]
    fn test_rejects_flat_format() {
        // Flat format (fields at top level without "properties" wrapper) should
        // be rejected with a clear error message.
        let json = json!({
            "variables": {
                "type": "object",
                "properties": {
                    "OPENAI_API_KEY": {
                        "type": ["string", "null"],
                        "env_var": "OPENAI_API_KEY"
                    }
                }
            },
            "steps": {}
        });

        let result = serde_json::from_value::<FlowSchema>(json);
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must be inside a \"properties\" wrapper"),
            "Expected clear error about properties wrapper, got: {err}"
        );
    }

    #[test]
    fn test_roundtrip_preserves_env_var() {
        let json = json!({
            "type": "object",
            "properties": {
                "variables": {
                    "type": "object",
                    "properties": {
                        "API_KEY": {
                            "type": "string",
                            "env_var": "MY_API_KEY",
                            "is_secret": true
                        }
                    }
                }
            }
        });

        let schema: FlowSchema = serde_json::from_value(json).unwrap();

        // Serialize back and deserialize again
        let serialized = serde_json::to_value(&schema).unwrap();
        let schema2: FlowSchema = serde_json::from_value(serialized).unwrap();
        assert!(schema2.variables.is_some());

        let var_schema = VariableSchema::from(schema2.variables.unwrap());
        assert_eq!(var_schema.env_var_name("API_KEY"), Some("MY_API_KEY"));
    }

    #[test]
    fn test_empty_schema_accepted() {
        // Empty object and null should both work
        let empty: FlowSchema = serde_json::from_value(json!({})).unwrap();
        assert!(empty.is_empty());

        let null: FlowSchema = serde_json::from_value(json!(null)).unwrap();
        assert!(null.is_empty());

        let type_only: FlowSchema = serde_json::from_value(json!({"type": "object"})).unwrap();
        assert!(type_only.is_empty());
    }
}
