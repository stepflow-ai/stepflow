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

//! Input schema inference from workflow constraints.
//!
//! This module provides functionality to infer a workflow's input schema by
//! collecting all `$input` references and the types they're expected to have.
//!
//! The inference works by:
//! 1. Finding all `$input` references in step inputs
//! 2. Looking up what type the component expects at each position
//! 3. Building a schema from those type constraints

use std::collections::HashMap;

use serde_json::json;
use stepflow_core::ValueExpr;
use stepflow_core::schema::SchemaRef;
use stepflow_core::values::JsonPath;
use stepflow_core::workflow::Flow;

use crate::ComponentSchemaProvider;

/// A constraint on the input schema, derived from usage of `$input` expressions.
#[derive(Debug, Clone)]
pub struct InputConstraint {
    /// The path being accessed (e.g., "message", "user.name")
    pub path: JsonPath,
    /// Whether this path is optional (accessed via $default or in optional context)
    pub is_optional: bool,
    /// The expected type at this path (from component input schema), if known
    pub expected_type: Option<SchemaRef>,
    /// The step that introduced this constraint (None for flow output)
    pub source_step: Option<String>,
}

/// A conflict between constraints on the same input path.
#[derive(Debug, Clone)]
pub struct InputConstraintConflict {
    /// The path that has conflicting constraints
    pub path: JsonPath,
    /// The steps that have conflicting expectations
    pub conflicting_steps: Vec<String>,
    /// Description of the conflict
    pub description: String,
}

/// Find conflicts between input constraints.
///
/// Two constraints conflict when they expect different types for the same path
/// and neither type is a subtype of the other.
pub fn find_constraint_conflicts(constraints: &[InputConstraint]) -> Vec<InputConstraintConflict> {
    use crate::{SubtypeResult, Type, is_subtype};

    let mut conflicts = Vec::new();

    // Group constraints by path (using the outer field)
    let mut by_path: HashMap<String, Vec<&InputConstraint>> = HashMap::new();
    for constraint in constraints {
        if let Some(field) = constraint.path.outer_field() {
            by_path
                .entry(field.to_string())
                .or_default()
                .push(constraint);
        }
    }

    // Check each path for conflicts
    for (path_field, path_constraints) in by_path {
        // Get constraints that have type information
        let typed_constraints: Vec<_> = path_constraints
            .iter()
            .filter_map(|c| {
                c.expected_type.as_ref().map(|t| {
                    let step = c
                        .source_step
                        .clone()
                        .unwrap_or_else(|| "<flow output>".to_string());
                    (step, t.clone())
                })
            })
            .collect();

        // Compare each pair of typed constraints
        for i in 0..typed_constraints.len() {
            for j in (i + 1)..typed_constraints.len() {
                let (step1, type1) = &typed_constraints[i];
                let (step2, type2) = &typed_constraints[j];

                // Check if the types are compatible
                let t1 = Type::Schema(type1.clone());
                let t2 = Type::Schema(type2.clone());

                let t1_sub_t2 = is_subtype(&t1, &t2);
                let t2_sub_t1 = is_subtype(&t2, &t1);

                // Types conflict if neither is a subtype of the other
                let is_conflict = !matches!(t1_sub_t2, SubtypeResult::Yes)
                    && !matches!(t2_sub_t1, SubtypeResult::Yes);

                if is_conflict {
                    // Create a path from the outer field for now
                    let path = JsonPath::from(format!("$.{}", path_field).as_str());

                    // Get concise type descriptions
                    let type1_desc = type1
                        .as_value()
                        .get("type")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            serde_json::to_string(type1.as_value())
                                .unwrap_or_else(|_| "?".to_string())
                        });
                    let type2_desc = type2
                        .as_value()
                        .get("type")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            serde_json::to_string(type2.as_value())
                                .unwrap_or_else(|_| "?".to_string())
                        });

                    conflicts.push(InputConstraintConflict {
                        path,
                        conflicting_steps: vec![step1.clone(), step2.clone()],
                        description: format!(
                            "step '{}' expects {} but step '{}' expects {}",
                            step1, type1_desc, step2, type2_desc
                        ),
                    });
                }
            }
        }
    }

    conflicts
}

/// Collect all input constraints from a flow, including type information from components.
///
/// This traverses all step inputs and uses the component schema provider to determine
/// what types are expected at each `$input` reference.
pub fn collect_input_constraints<P: ComponentSchemaProvider>(
    flow: &Flow,
    provider: &P,
) -> Vec<InputConstraint> {
    let mut constraints = Vec::new();

    // Collect from step inputs with component type context
    for step in flow.steps() {
        // Get the component's input schema to determine expected types
        let component_input_schema = provider.get_input_schema(&step.component);
        let schema_ref = component_input_schema.as_ref().and_then(|t| t.as_schema());

        collect_from_expr_with_context(
            &step.input,
            &mut constraints,
            false,
            schema_ref,
            Some(&step.id),
        );
    }

    // Collect from flow output (no type context for these)
    collect_from_expr_with_context(flow.output(), &mut constraints, false, None, None);

    constraints
}

/// Recursively collect input constraints from an expression, tracking expected type context.
fn collect_from_expr_with_context(
    expr: &ValueExpr,
    constraints: &mut Vec<InputConstraint>,
    in_optional: bool,
    expected_schema: Option<&SchemaRef>,
    source_step: Option<&str>,
) {
    match expr {
        ValueExpr::Input { input } => {
            // When we find an $input reference, we know what type is expected
            // The expected_schema tells us what the component wants at this position
            constraints.push(InputConstraint {
                path: input.clone(),
                is_optional: in_optional,
                expected_type: expected_schema.cloned(),
                source_step: source_step.map(|s| s.to_string()),
            });
        }
        ValueExpr::Step { .. } => {
            // Step references don't contribute to input constraints
        }
        ValueExpr::Variable { default, .. } => {
            // Variable default values might contain input references
            if let Some(default_expr) = default {
                collect_from_expr_with_context(default_expr, constraints, true, None, source_step);
            }
        }
        ValueExpr::EscapedLiteral { .. } | ValueExpr::Literal(_) => {
            // Literals don't contain input references
        }
        ValueExpr::If {
            condition,
            then,
            else_expr,
        } => {
            // Condition doesn't have a specific type expectation
            collect_from_expr_with_context(condition, constraints, in_optional, None, source_step);
            // Then and else branches inherit the expected type
            collect_from_expr_with_context(
                then,
                constraints,
                in_optional,
                expected_schema,
                source_step,
            );
            if let Some(else_e) = else_expr {
                collect_from_expr_with_context(
                    else_e,
                    constraints,
                    in_optional,
                    expected_schema,
                    source_step,
                );
            }
        }
        ValueExpr::Coalesce { values } => {
            // All values in a coalesce are optional except the last one
            // They all have the same expected type
            let len = values.len();
            for (i, v) in values.iter().enumerate() {
                let is_optional_here = in_optional || i < len - 1;
                collect_from_expr_with_context(
                    v,
                    constraints,
                    is_optional_here,
                    expected_schema,
                    source_step,
                );
            }
        }
        ValueExpr::Array(items) => {
            // For arrays, we'd need to look at the array's item schema
            // For now, just traverse without type context
            for item in items {
                collect_from_expr_with_context(item, constraints, in_optional, None, source_step);
            }
        }
        ValueExpr::Object(props) => {
            // For objects, look up each property in the expected schema
            for (prop_name, value) in props {
                let prop_schema = expected_schema.and_then(|s| {
                    let v = s.as_value();
                    v.get("properties")
                        .and_then(|props| props.get(prop_name))
                        .map(|prop_value| SchemaRef::from(prop_value.clone()))
                });
                collect_from_expr_with_context(
                    value,
                    constraints,
                    in_optional,
                    prop_schema.as_ref(),
                    source_step,
                );
            }
        }
    }
}

/// A trie node for building nested schemas from path constraints.
#[derive(Debug, Default)]
struct SchemaTrieNode {
    /// Type constraint at this node (if the path ends here)
    leaf_type: Option<serde_json::Value>,
    /// Whether this path is required (any non-optional access)
    is_required: bool,
    /// Child nodes for nested paths
    children: HashMap<String, SchemaTrieNode>,
    /// Source step for error reporting (first constraint that set the leaf type)
    source_step: Option<String>,
}

impl SchemaTrieNode {
    fn new() -> Self {
        Self::default()
    }

    /// Insert a constraint into the trie.
    /// Returns a conflict description if inserting causes a structural conflict.
    fn insert(
        &mut self,
        parts: &[stepflow_core::values::PathPart],
        constraint: &InputConstraint,
    ) -> Option<String> {
        use stepflow_core::values::PathPart;

        if parts.is_empty() {
            // This is a leaf node - set or merge the type constraint
            if let Some(new_schema) = constraint
                .expected_type
                .as_ref()
                .map(|t| t.as_value().clone())
            {
                if let Some(existing) = &mut self.leaf_type {
                    // Merge: take description from new if existing doesn't have one
                    if existing.get("description").is_none()
                        && let Some(desc) = new_schema.get("description")
                    {
                        existing["description"] = desc.clone();
                    }
                    // Could also merge other properties like title, examples, etc.
                    if existing.get("title").is_none()
                        && let Some(title) = new_schema.get("title")
                    {
                        existing["title"] = title.clone();
                    }
                } else {
                    self.leaf_type = Some(new_schema);
                    self.source_step = constraint.source_step.clone();
                }
            }

            // Update required status - required if any access is non-optional
            if !constraint.is_optional {
                self.is_required = true;
            }

            // Check for structural conflict: leaf type is set but we have children
            if self.leaf_type.is_some() && !self.children.is_empty() {
                return Some("path has both a primitive type and nested properties".to_string());
            }
            return None;
        }

        // Get the first part of the remaining path
        let (field_name, rest) = match &parts[0] {
            PathPart::Field(name) => (name.clone(), &parts[1..]),
            PathPart::IndexStr(name) => (name.clone(), &parts[1..]),
            PathPart::Index(_) => {
                // Array index access - we don't handle this well yet
                // Just treat the whole remaining path as a single access
                return None;
            }
        };

        // Check for structural conflict: we have a leaf type but are trying to add children
        if self.leaf_type.is_some() {
            return Some(format!(
                "path is constrained as a primitive but '{}' accesses nested property '{}'",
                constraint.source_step.as_deref().unwrap_or("<unknown>"),
                field_name
            ));
        }

        // Insert into child node
        let child = self.children.entry(field_name).or_default();
        child.insert(rest, constraint)
    }

    /// Build a JSON Schema from this trie node.
    fn to_schema(&self) -> serde_json::Value {
        if let Some(leaf_type) = &self.leaf_type {
            // This is a leaf node with a type constraint
            return leaf_type.clone();
        }

        if self.children.is_empty() {
            // No type constraint and no children - use empty schema (any type)
            return json!({});
        }

        // This is an object with nested properties
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for (prop_name, child) in &self.children {
            properties.insert(prop_name.clone(), child.to_schema());
            if child.is_required {
                required.push(prop_name.clone());
            }
        }

        // Sort required for deterministic output
        required.sort();

        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }
}

/// Build an input schema from collected constraints.
///
/// This creates a JSON Schema with properties for each accessed path,
/// using the expected types from component input schemas when available.
/// Nested paths like `$.foo.bar` are properly handled to create nested object schemas.
pub fn build_input_schema(constraints: &[InputConstraint]) -> Option<SchemaRef> {
    if constraints.is_empty() {
        return None;
    }

    // Build trie from constraints
    let mut root = SchemaTrieNode::new();

    for constraint in constraints {
        let parts = constraint.path.parts();
        if let Some(_conflict) = root.insert(parts, constraint) {
            // TODO: Could collect these conflicts and return them
            // For now, we just continue and use the first type we saw
        }
    }

    if root.children.is_empty() && root.leaf_type.is_none() {
        return None;
    }

    // Build the schema from the trie
    let schema_value = root.to_schema();

    // If the root itself has a leaf type (shouldn't happen for valid flows), return it
    // Otherwise, wrap in object schema if not already
    let final_schema = if root.leaf_type.is_some() {
        schema_value
    } else {
        // Ensure we have an object schema at the root
        let mut obj = schema_value;
        if obj.get("type").is_none() {
            obj["type"] = json!("object");
        }
        obj
    };

    Some(SchemaRef::from(final_schema))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;
    use stepflow_core::workflow::{Component, FlowBuilder, StepBuilder};

    /// A test schema provider that returns specified schemas
    struct TestProvider {
        input_schemas: HashMap<String, Type>,
    }

    impl TestProvider {
        fn new() -> Self {
            Self {
                input_schemas: HashMap::new(),
            }
        }

        fn with_input_schema(mut self, component: &str, schema: serde_json::Value) -> Self {
            self.input_schemas
                .insert(component.to_string(), Type::Schema(SchemaRef::from(schema)));
            self
        }
    }

    impl ComponentSchemaProvider for TestProvider {
        fn get_input_schema(&self, component: &Component) -> Option<Type> {
            self.input_schemas.get(component.path()).cloned()
        }

        fn get_output_schema(&self, _component: &Component) -> Option<Type> {
            None
        }

        fn infer_output_schema(&self, _component: &Component, _input: &Type) -> Option<Type> {
            None
        }
    }

    #[test]
    fn test_collect_simple_input() {
        let step = StepBuilder::new("s1")
            .component("/test")
            .input(ValueExpr::Input {
                input: JsonPath::from("$.message"),
            })
            .build();

        let flow = FlowBuilder::new()
            .step(step)
            .output(ValueExpr::Step {
                step: "s1".to_string(),
                path: JsonPath::new(),
            })
            .build();

        let provider = TestProvider::new();
        let constraints = collect_input_constraints(&flow, &provider);
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].path.to_string(), "$.message");
        assert!(!constraints[0].is_optional);
    }

    #[test]
    fn test_collect_with_type_context() {
        let step = StepBuilder::new("s1")
            .component("/test")
            .input(ValueExpr::Object(vec![(
                "user_prompt".to_string(),
                ValueExpr::Input {
                    input: JsonPath::from("$.message"),
                },
            )]))
            .build();

        let flow = FlowBuilder::new()
            .step(step)
            .output(ValueExpr::Step {
                step: "s1".to_string(),
                path: JsonPath::new(),
            })
            .build();

        // Provider that says /test expects user_prompt to be a string
        let provider = TestProvider::new().with_input_schema(
            "/test",
            json!({
                "type": "object",
                "properties": {
                    "user_prompt": { "type": "string" }
                }
            }),
        );

        let constraints = collect_input_constraints(&flow, &provider);
        assert_eq!(constraints.len(), 1);
        assert!(constraints[0].expected_type.is_some());

        // The expected type should be "string"
        let expected = constraints[0].expected_type.as_ref().unwrap();
        assert_eq!(
            expected.as_value().get("type").and_then(|v| v.as_str()),
            Some("string")
        );
    }

    #[test]
    fn test_build_schema_with_types() {
        let constraints = vec![InputConstraint {
            path: JsonPath::from("$.message"),
            is_optional: false,
            expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
            source_step: Some("step1".to_string()),
        }];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        // Check that message property has string type
        let msg_schema = value
            .get("properties")
            .and_then(|p| p.get("message"))
            .expect("should have message property");
        assert_eq!(
            msg_schema.get("type").and_then(|t| t.as_str()),
            Some("string")
        );

        // Check required
        let required = value
            .get("required")
            .and_then(|r| r.as_array())
            .expect("required should be an array");
        assert!(required.iter().any(|v| v.as_str() == Some("message")));
    }

    #[test]
    fn test_build_schema_without_types() {
        let constraints = vec![InputConstraint {
            path: JsonPath::from("$.message"),
            is_optional: false,
            expected_type: None,
            source_step: Some("step1".to_string()),
        }];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        // Message property should have empty schema (any type)
        let msg_schema = value
            .get("properties")
            .and_then(|p| p.get("message"))
            .expect("should have message property");
        assert!(
            msg_schema
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_optional_input() {
        let constraints = vec![InputConstraint {
            path: JsonPath::from("$.optional_field"),
            is_optional: true,
            expected_type: None,
            source_step: Some("step1".to_string()),
        }];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        // Property should exist but not be required
        assert!(
            value
                .get("properties")
                .and_then(|p| p.get("optional_field"))
                .is_some()
        );

        let required = value
            .get("required")
            .and_then(|r| r.as_array())
            .expect("required should be an array");
        assert!(
            !required
                .iter()
                .any(|v| v.as_str() == Some("optional_field"))
        );
    }

    #[test]
    fn test_find_conflicts_no_conflict() {
        use super::find_constraint_conflicts;

        // Both steps expect string - no conflict
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.message"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.message"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step2".to_string()),
            },
        ];

        let conflicts = find_constraint_conflicts(&constraints);
        assert!(conflicts.is_empty(), "Should have no conflicts");
    }

    #[test]
    fn test_find_conflicts_type_mismatch() {
        use super::find_constraint_conflicts;

        // step1 expects string, step2 expects number - conflict!
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.foo"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.foo"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "number"}))),
                source_step: Some("step2".to_string()),
            },
        ];

        let conflicts = find_constraint_conflicts(&constraints);
        assert_eq!(conflicts.len(), 1, "Should have one conflict");

        let conflict = &conflicts[0];
        assert!(conflict.conflicting_steps.contains(&"step1".to_string()));
        assert!(conflict.conflicting_steps.contains(&"step2".to_string()));
        assert!(conflict.description.contains("string"));
        assert!(conflict.description.contains("number"));
    }

    #[test]
    fn test_find_conflicts_different_paths_no_conflict() {
        use super::find_constraint_conflicts;

        // Different paths - no conflict even with different types
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.foo"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.bar"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "number"}))),
                source_step: Some("step2".to_string()),
            },
        ];

        let conflicts = find_constraint_conflicts(&constraints);
        assert!(conflicts.is_empty(), "Different paths should not conflict");
    }

    #[test]
    fn test_build_nested_schema() {
        // Test that nested paths produce nested object schemas
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.user.name"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.user.age"),
                is_optional: true,
                expected_type: Some(SchemaRef::from(json!({"type": "number"}))),
                source_step: Some("step2".to_string()),
            },
        ];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        // Check that user is an object
        let user_schema = value
            .get("properties")
            .and_then(|p| p.get("user"))
            .expect("should have user property");
        assert_eq!(
            user_schema.get("type").and_then(|t| t.as_str()),
            Some("object")
        );

        // Check nested properties
        let user_props = user_schema
            .get("properties")
            .expect("user should have properties");
        assert!(user_props.get("name").is_some());
        assert!(user_props.get("age").is_some());

        // Check required - user.name is required, user.age is optional
        let user_required = user_schema
            .get("required")
            .and_then(|r| r.as_array())
            .expect("required should be array");
        assert!(user_required.iter().any(|v| v.as_str() == Some("name")));
        assert!(!user_required.iter().any(|v| v.as_str() == Some("age")));
    }

    #[test]
    fn test_build_sibling_paths() {
        // Test that sibling paths are handled correctly
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.foo"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.bar"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "number"}))),
                source_step: Some("step2".to_string()),
            },
        ];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        // Both foo and bar should be present
        let props = value.get("properties").expect("should have properties");
        assert!(props.get("foo").is_some());
        assert!(props.get("bar").is_some());

        // Both should be required
        let required = value
            .get("required")
            .and_then(|r| r.as_array())
            .expect("required should be array");
        assert!(required.iter().any(|v| v.as_str() == Some("foo")));
        assert!(required.iter().any(|v| v.as_str() == Some("bar")));
    }

    #[test]
    fn test_description_merging_first_has_none() {
        // When first constraint has no description but second does, take the second's description
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.message"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "string"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.message"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({
                    "type": "string",
                    "description": "The message content"
                }))),
                source_step: Some("step2".to_string()),
            },
        ];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        let msg_schema = value
            .get("properties")
            .and_then(|p| p.get("message"))
            .expect("should have message property");

        // Should have the description from step2
        assert_eq!(
            msg_schema.get("description").and_then(|d| d.as_str()),
            Some("The message content")
        );
    }

    #[test]
    fn test_description_merging_first_has_description() {
        // When first constraint already has a description, keep it
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.message"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({
                    "type": "string",
                    "description": "First description"
                }))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.message"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({
                    "type": "string",
                    "description": "Second description"
                }))),
                source_step: Some("step2".to_string()),
            },
        ];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        let msg_schema = value
            .get("properties")
            .and_then(|p| p.get("message"))
            .expect("should have message property");

        // Should keep the first description
        assert_eq!(
            msg_schema.get("description").and_then(|d| d.as_str()),
            Some("First description")
        );
    }

    #[test]
    fn test_title_merging() {
        // Title should also be merged from later constraints if first doesn't have it
        let constraints = vec![
            InputConstraint {
                path: JsonPath::from("$.data"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({"type": "object"}))),
                source_step: Some("step1".to_string()),
            },
            InputConstraint {
                path: JsonPath::from("$.data"),
                is_optional: false,
                expected_type: Some(SchemaRef::from(json!({
                    "type": "object",
                    "title": "DataPayload"
                }))),
                source_step: Some("step2".to_string()),
            },
        ];

        let schema = build_input_schema(&constraints);
        assert!(schema.is_some());

        let schema = schema.unwrap();
        let value = schema.as_value();

        let data_schema = value
            .get("properties")
            .and_then(|p| p.get("data"))
            .expect("should have data property");

        // Should have the title from step2
        assert_eq!(
            data_schema.get("title").and_then(|t| t.as_str()),
            Some("DataPayload")
        );
    }
}
