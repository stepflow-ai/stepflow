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

use super::*;
use crate::diagnostics::DiagnosticMessage;
use serde_json::json;
use stepflow_core::{
    values::ValueTemplate,
    workflow::{FlowBuilder, JsonPath, Step, StepBuilder},
};

fn create_test_step(id: &str, input: serde_json::Value) -> Step {
    StepBuilder::mock_step(id).input_json(input).build()
}

#[test]
fn test_valid_workflow() {
    let flow = FlowBuilder::test_flow()
        .description("A test workflow")
        .steps(vec![
            create_test_step("step1", json!({"$from": {"workflow": "input"}})),
            create_test_step("step2", json!({"$from": {"step": "step1"}})),
        ])
        .output(ValueTemplate::step_ref("step2", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert_eq!(diagnostics.num_fatal, 0, "Expected no fatal diagnostics");
}

#[test]
fn test_forward_reference_error() {
    let flow = FlowBuilder::test_flow()
        .steps(vec![
            create_test_step("step1", json!({"$from": {"step": "step2"}})), // Forward reference
            create_test_step("step2", json!({"$from": {"workflow": "input"}})),
        ])
        .output(ValueTemplate::step_ref("step2", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();

    assert!(diagnostics.num_fatal > 0, "Expected fatal diagnostics");
    assert!(diagnostics.diagnostics.iter().any(|d| matches!(
        d.message(),
        DiagnosticMessage::UndefinedStepReference { .. }
    )));
}

#[test]
fn test_duplicate_step_ids() {
    let flow = FlowBuilder::test_flow()
        .steps(vec![
            create_test_step("step1", json!({"$from": {"workflow": "input"}})),
            create_test_step("step1", json!({"$from": {"workflow": "input"}})), // Duplicate ID
        ])
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert!(diagnostics.num_fatal > 0, "Expected fatal diagnostics");
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::DuplicateStepId { .. }))
    );
}

#[test]
fn test_self_reference() {
    let flow = FlowBuilder::test_flow()
        .steps(vec![create_test_step(
            "step1",
            json!({"$from": {"step": "step1"}}),
        )])
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert!(diagnostics.num_fatal > 0, "Expected fatal diagnostics");
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::SelfReference { .. }))
    );
}

#[test]
fn test_unreachable_step() {
    let flow = FlowBuilder::test_flow()
        .steps(vec![
            create_test_step("step1", json!({"$from": {"workflow": "input"}})),
            create_test_step("step2", json!({"$from": {"workflow": "input"}})), // Not referenced
        ])
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert!(diagnostics.num_warning > 0, "Expected warnings");
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::UnreachableStep { .. }))
    );
}

#[test]
fn test_workflow_with_no_name_and_description() {
    let flow = FlowBuilder::new() // No name/description
        .steps(vec![create_test_step(
            "step1",
            json!({"$from": {"workflow": "input"}}),
        )])
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert_eq!(diagnostics.num_fatal, 0, "Expected no fatal diagnostics"); // These are warnings, not fatal
    assert!(diagnostics.num_warning >= 2, "Expected at least 2 warnings"); // name + description + possibly others
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::MissingFlowName))
    );
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::MissingFlowDescription))
    );
}

#[test]
fn test_empty_component_name() {
    let flow = FlowBuilder::test_flow()
        .step(
            StepBuilder::new("step1")
                .component("") // Empty builtin name
                .input(ValueTemplate::workflow_input(JsonPath::default()))
                .build(),
        )
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert_eq!(diagnostics.num_fatal, 0, "Expected no fatal diagnostics");
    assert!(diagnostics.num_error > 0, "Expected error diagnostics");
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::InvalidComponent { .. }))
    );
}

#[test]
fn test_valid_builtin_component() {
    let flow = FlowBuilder::test_flow()
        .step(
            StepBuilder::builtin_step("step1", "eval")
                .input(ValueTemplate::workflow_input(JsonPath::default()))
                .build(),
        )
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert_eq!(diagnostics.num_fatal, 0, "Expected no fatal diagnostics");
    assert_eq!(
        diagnostics.num_error, 0,
        "Expected no error diagnostics for valid builtin"
    );
    // Should have warnings but no errors for valid builtin components
}

#[test]
fn test_combined_validation() {
    use std::collections::HashMap;
    use stepflow_plugin::routing::{RouteRule, RoutingConfig};

    let flow = FlowBuilder::test_flow()
        .description("A test workflow")
        .step(create_test_step(
            "step1",
            json!({"$from": {"workflow": "input"}}),
        ))
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let mut plugins = IndexMap::new();
    plugins.insert("builtin".to_string(), ());

    let mut routes = HashMap::new();
    routes.insert(
        "/{*component}".to_string(),
        vec![RouteRule {
            plugin: "builtin".into(),
            conditions: vec![],
            component_allow: None,
            component_deny: None,
            component: None,
        }],
    );

    let routing = RoutingConfig { routes };

    let diagnostics = validate_with_config(&flow, &plugins, &routing).unwrap();
    assert_eq!(diagnostics.num_fatal, 0, "Expected no fatal diagnostics");
    assert_eq!(diagnostics.num_error, 0, "Expected no error diagnostics");
}

#[test]
fn test_undefined_required_variable() {
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::workflow::VariableSchema;

    // Create a flow with variable schema
    let var_schema_json = json!({
        "type": "object",
        "properties": {
            "api_key": {
                "type": "string",
                "description": "API key"
            }
        },
        "required": ["api_key"]
    });
    let schema = SchemaRef::parse_json(&var_schema_json.to_string()).unwrap();
    let var_schema = VariableSchema::from(schema);

    let flow = FlowBuilder::new()
        .name("test_flow")
        .variables(var_schema)
        .step(
            StepBuilder::mock_step("step1")
                .input_json(json!({
                    "key": {"$from": {"variable": "undefined_var"}}
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert!(
        diagnostics.num_error > 0,
        "Expected error diagnostics for undefined required variable"
    );
    assert!(diagnostics.diagnostics.iter().any(|d| matches!(
        d.message(),
        DiagnosticMessage::UndefinedRequiredVariable { .. }
    )));
}

#[test]
fn test_valid_variable_reference() {
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::workflow::VariableSchema;

    // Create a flow with variable schema
    let var_schema_json = json!({
        "type": "object",
        "properties": {
            "api_key": {
                "type": "string",
                "description": "API key"
            }
        },
        "required": ["api_key"]
    });
    let schema = SchemaRef::parse_json(&var_schema_json.to_string()).unwrap();
    let var_schema = VariableSchema::from(schema);

    let flow = FlowBuilder::new()
        .name("test_flow")
        .variables(var_schema)
        .step(
            StepBuilder::mock_step("step1")
                .input_json(json!({
                    "key": {"$from": {"variable": "api_key"}}
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    // Should not have errors for valid variable reference
    assert!(!diagnostics.diagnostics.iter().any(|d| matches!(
        d.message(),
        DiagnosticMessage::UndefinedRequiredVariable { .. }
    )));
}

#[test]
fn test_variable_reference_without_schema() {
    // Flow without variable schema
    let flow = FlowBuilder::new()
        .name("test_flow")
        .step(
            StepBuilder::mock_step("step1")
                .input_json(json!({
                    "key": {"$from": {"variable": "some_var"}}
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::MissingVariableSchema))
    );
}

#[test]
fn test_undefined_optional_variable_with_default() {
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::workflow::VariableSchema;

    // Create a flow with variable schema
    let var_schema_json = json!({
        "type": "object",
        "properties": {
            "api_key": {
                "type": "string"
            }
        },
        "required": ["api_key"]
    });
    let schema = SchemaRef::parse_json(&var_schema_json.to_string()).unwrap();
    let var_schema = VariableSchema::from(schema);

    let flow = FlowBuilder::new()
        .name("test_flow")
        .variables(var_schema)
        .step(
            StepBuilder::mock_step("step1")
                .input_json(json!({
                    "key": {
                        "$from": {
                            "variable": "undefined_var",
                            "default": "fallback_value"
                        }
                    }
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref("step1", JsonPath::default()))
        .build();

    let diagnostics = validate(&flow).unwrap();
    // Should have error (not fatal) for undefined variable with inline default
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .any(|d| matches!(d.message(), DiagnosticMessage::UndefinedVariable { .. }))
    );
}

#[test]
fn test_literal_subflow_validation() {
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::workflow::VariableSchema;

    // Create parent flow with variables
    let parent_var_schema_json = json!({
        "type": "object",
        "properties": {
            "api_key": {"type": "string"},
            "endpoint": {"type": "string"}
        },
        "required": ["api_key", "endpoint"]
    });
    let parent_schema = SchemaRef::parse_json(&parent_var_schema_json.to_string()).unwrap();
    let parent_var_schema = VariableSchema::from(parent_schema);

    // Create a valid subflow that uses parent variables
    let subflow_json = json!({
        "schema": "https://stepflow.org/schemas/v1/flow.json",
        "name": "subflow",
        "variables": {
            "type": "object",
            "properties": {
                "api_key": {"type": "string"}
            },
            "required": ["api_key"]
        },
        "steps": [{
            "id": "sub_step1",
            "component": "/mock/test",
            "input": {
                "key": {"$from": {"variable": "api_key"}}
            }
        }],
        "output": {"$from": {"step": "sub_step1"}}
    });

    // Create parent flow with put_blob step containing literal subflow
    let flow = FlowBuilder::new()
        .name("parent_flow")
        .variables(parent_var_schema)
        .step(
            StepBuilder::new("create_subflow")
                .component("/put_blob")
                .input_json(json!({
                    "blob_type": "flow",
                    "data": {
                        "$literal": subflow_json
                    },
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref(
            "create_subflow",
            JsonPath::default(),
        ))
        .build();

    let diagnostics = validate(&flow).unwrap();

    assert_eq!(
        diagnostics.num_fatal, 0,
        "Expected no fatal diagnostics for valid subflow"
    );
    assert_eq!(
        diagnostics.num_error, 0,
        "Expected no error diagnostics for valid subflow"
    );
}

#[test]
fn test_subflow_undefined_variable() {
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::workflow::VariableSchema;

    // Create parent flow with limited variables
    let parent_var_schema_json = json!({
        "type": "object",
        "properties": {
            "api_key": {"type": "string"}
        },
        "required": ["api_key"]
    });
    let parent_schema = SchemaRef::parse_json(&parent_var_schema_json.to_string()).unwrap();
    let parent_var_schema = VariableSchema::from(parent_schema);

    // Create subflow that uses undefined variable
    let subflow_json = json!({
        "schema": "https://stepflow.org/schemas/v1/flow.json",
        "name": "subflow",
        "variables": {
            "type": "object",
            "properties": {
                "api_key": {"type": "string"},
                "undefined_var": {"type": "string"}
            },
            "required": ["api_key", "undefined_var"]
        },
        "steps": [{
            "id": "sub_step1",
            "component": "/mock/test",
            "input": {
                "key": {"$from": {"variable": "undefined_var"}}
            }
        }],
        "output": {"$from": {"step": "sub_step1"}}
    });

    let flow = FlowBuilder::new()
        .name("parent_flow")
        .variables(parent_var_schema)
        .step(
            StepBuilder::new("create_subflow")
                .component("/put_blob")
                .input_json(json!({
                    "blob_type": "flow",
                    "data": {
                        "$literal": subflow_json
                    },
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref(
            "create_subflow",
            JsonPath::default(),
        ))
        .build();

    let diagnostics = validate(&flow).unwrap();

    // Subflow variables don't need to match parent variables since they can be
    // provided at execution time, so this should validate successfully
    assert_eq!(
        diagnostics.num_error, 0,
        "Should not error - subflow variables can be provided at execution time"
    );
}

#[test]
fn test_subflow_with_validation_errors() {
    use stepflow_core::schema::SchemaRef;
    use stepflow_core::workflow::VariableSchema;

    // Create parent flow with variables
    let parent_var_schema_json = json!({
        "type": "object",
        "properties": {
            "api_key": {"type": "string"}
        },
        "required": ["api_key"]
    });
    let parent_schema = SchemaRef::parse_json(&parent_var_schema_json.to_string()).unwrap();
    let parent_var_schema = VariableSchema::from(parent_schema);

    // Create subflow with internal validation errors (forward reference)
    let subflow_json = json!({
        "schema": "https://stepflow.org/schemas/v1/flow.json",
        "name": "subflow",
        "steps": [
            {
                "id": "step1",
                "component": "/mock/test",
                "input": {"$from": {"step": "step2"}}  // Forward reference!
            },
            {
                "id": "step2",
                "component": "/mock/test",
                "input": {}
            }
        ],
        "output": {"$from": {"step": "step2"}}
    });

    let flow = FlowBuilder::new()
        .name("parent_flow")
        .variables(parent_var_schema)
        .step(
            StepBuilder::new("create_subflow")
                .component("/put_blob")
                .input_json(json!({
                    "blob_type": "flow",
                    "data": {
                        "$literal": subflow_json
                    },
                }))
                .build(),
        )
        .output(ValueTemplate::step_ref(
            "create_subflow",
            JsonPath::default(),
        ))
        .build();

    let diagnostics = validate(&flow).unwrap();

    assert!(
        diagnostics.num_fatal + diagnostics.num_error > 0,
        "Expected errors from subflow validation"
    );

    let error = diagnostics.iter().find(|d| {
        matches!(
            d.message(),
            DiagnosticMessage::UndefinedStepReference { .. }
        )
    });
    assert!(
        error.is_some(),
        "Expected error about invalid reference in the subflow"
    );
    let error = error.unwrap();
    assert_eq!(
        error.path,
        make_path!("steps", 0, "data", "$literal", "steps", 0, "input")
    );
    assert_eq!(error.text, "Step 'step1' references undefined step 'step2'");
}
