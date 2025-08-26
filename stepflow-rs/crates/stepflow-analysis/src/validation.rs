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
use std::collections::HashSet;
use stepflow_core::workflow::{BaseRef, Component, Expr, Flow, Step, ValueTemplate, WorkflowRef};

use crate::Result;
use crate::diagnostics::{DiagnosticMessage, Diagnostics};

/// Validates a workflow and collects all diagnostics
pub fn validate_workflow(flow: &Flow) -> Result<Diagnostics> {
    let mut diagnostics = Diagnostics::new();

    // Validate workflow structure
    validate_workflow_structure(flow, &mut diagnostics);

    // Validate step ordering and references
    validate_step_ordering_and_references(flow, &mut diagnostics);

    // Validate workflow output
    let all_step_ids: HashSet<String> = flow.steps().iter().map(|s| s.id.clone()).collect();
    validate_references(
        flow.output(),
        &["output".to_string()],
        &all_step_ids,
        "workflow_output",
        &mut diagnostics,
    );

    // Check for unreachable steps
    detect_unreachable_steps(flow, &mut diagnostics)?;

    Ok(diagnostics)
}

/// Validate basic workflow structure
fn validate_workflow_structure(flow: &Flow, diagnostics: &mut Diagnostics) {
    // Check for duplicate step IDs
    let mut seen_ids = HashSet::new();
    for (index, step) in flow.steps().iter().enumerate() {
        if !seen_ids.insert(&step.id) {
            diagnostics.add(
                DiagnosticMessage::DuplicateStepId {
                    step_id: step.id.clone(),
                },
                vec!["steps".to_string(), index.to_string(), "id".to_string()],
            );
        }
    }

    // Check for empty step IDs
    for (index, step) in flow.steps().iter().enumerate() {
        if step.id.trim().is_empty() {
            diagnostics.add(
                DiagnosticMessage::EmptyStepId,
                vec!["steps".to_string(), index.to_string(), "id".to_string()],
            );
        }
    }

    // Warn if workflow has no name
    if flow.name().is_none() || flow.name().unwrap().trim().is_empty() {
        diagnostics.add(
            DiagnosticMessage::MissingWorkflowName,
            vec!["name".to_string()],
        );
    }

    // Warn if workflow has no description
    if flow.description().is_none() {
        diagnostics.add(
            DiagnosticMessage::MissingWorkflowDescription,
            vec!["description".to_string()],
        );
    }
}

/// Validate step ordering and references - steps can only reference earlier steps
fn validate_step_ordering_and_references(flow: &Flow, diagnostics: &mut Diagnostics) {
    let mut available_steps = HashSet::new();

    for (index, step) in flow.steps().iter().enumerate() {
        // Validate this step only references previously defined steps
        validate_step_references(step, index, &available_steps, diagnostics);

        // Add this step to available set for future steps
        available_steps.insert(step.id.clone());
    }
}

/// Validate that a step only references available (previously defined) steps
fn validate_step_references(
    step: &Step,
    step_index: usize,
    available_steps: &HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    let step_path = vec!["steps".to_string(), step_index.to_string()];

    // Validate step input references
    let mut input_path = step_path.clone();
    input_path.push("input".to_string());
    validate_references(
        &step.input,
        &input_path,
        available_steps,
        &step.id,
        diagnostics,
    );

    // Validate skip condition references
    if let Some(skip_if) = &step.skip_if {
        let mut skip_path = step_path.clone();
        skip_path.push("skip_if".to_string());
        validate_expression_references(skip_if, &skip_path, available_steps, &step.id, diagnostics);
    }

    // Validate component
    let mut component_path = step_path.clone();
    component_path.push("component".to_string());
    validate_component(&step.component, &component_path, diagnostics);

    // TODO: Warn about mock components. We'll need to look at the plugin
    // definitinos to find out which ones are actually registered as mocsk.
}

/// Validate references within an expression
fn validate_expression_references(
    expr: &Expr,
    path: &[String],
    available_steps: &HashSet<String>,
    current_step_id: &str,
    diagnostics: &mut Diagnostics,
) {
    match expr {
        Expr::Ref {
            from,
            path: field_path,
            ..
        } => match from {
            BaseRef::Step { step } => {
                // Check for self-reference
                if current_step_id == step {
                    diagnostics.add(
                        DiagnosticMessage::SelfReference {
                            step_id: step.clone(),
                        },
                        path.to_vec(),
                    );
                    return;
                }

                // Check if step exists and is available (defined earlier)
                if !available_steps.contains(step) {
                    diagnostics.add(
                        DiagnosticMessage::UndefinedStepReference {
                            from_step: Some(current_step_id.to_string()),
                            referenced_step: step.clone(),
                        },
                        path.to_vec(),
                    );
                    return;
                }

                // Generate ignored diagnostic about potential field access issues (when we don't have schema info)
                if !field_path.is_empty() {
                    diagnostics.add(
                        DiagnosticMessage::UnvalidatedFieldAccess {
                            step_id: step.clone(),
                            field: field_path.to_string(),
                            reason: "no output schema available".to_string(),
                        },
                        path.to_vec(),
                    );
                }
            }
            BaseRef::Workflow(WorkflowRef::Input) => {
                // Workflow input reference is always valid
                // Generate ignored diagnostic about unvalidated field access on workflow input
                if !field_path.is_empty() {
                    diagnostics.add(
                        DiagnosticMessage::UnvalidatedFieldAccess {
                            step_id: "workflow_input".to_string(),
                            field: field_path.to_string(),
                            reason: "no input schema available".to_string(),
                        },
                        path.to_vec(),
                    );
                }
            }
        },
        Expr::EscapedLiteral { .. } | Expr::Literal(_) => {
            // Literals are always valid
        }
    }
}

/// Detect unreachable steps (steps that no other step or output depends on)
fn detect_unreachable_steps(flow: &Flow, diagnostics: &mut Diagnostics) -> Result<()> {
    let mut referenced_steps = HashSet::new();

    // Collect steps referenced by other steps
    for step in flow.steps() {
        collect_step_dependencies(&step.input, &mut referenced_steps)?;
        if let Some(skip_if) = &step.skip_if {
            collect_expression_dependencies(skip_if, &mut referenced_steps);
        }
    }

    // Collect steps referenced by workflow output
    collect_step_dependencies(flow.output(), &mut referenced_steps)?;

    // Find unreachable steps
    for (index, step) in flow.steps().iter().enumerate() {
        if !referenced_steps.contains(&step.id) {
            diagnostics.add(
                DiagnosticMessage::UnreachableStep {
                    step_id: step.id.clone(),
                },
                vec!["steps".to_string(), index.to_string()],
            );
        }
    }

    Ok(())
}

/// Collect step dependencies from an expression
fn collect_expression_dependencies(expr: &Expr, dependencies: &mut HashSet<String>) {
    match expr {
        Expr::Ref { from, .. } => {
            if let BaseRef::Step { step } = from {
                dependencies.insert(step.clone());
            }
        }
        Expr::EscapedLiteral { .. } | Expr::Literal(_) => {}
    }
}

/// Validate a component URL
fn validate_component(component: &Component, path: &[String], diagnostics: &mut Diagnostics) {
    let path_str = component.path();
    if !path_str.starts_with('/') {
        let error = Cow::Borrowed("Component path must start with '/'");

        // Extract step_id from path for backwards compatibility with DiagnosticMessage
        let step_id = path.get(1).unwrap_or(&"unknown".to_string()).clone();
        diagnostics.add(
            DiagnosticMessage::InvalidComponent {
                step_id,
                component: path_str.to_string(),
                error,
            },
            path.to_vec(),
        );
    }
}

/// Validate all references within a ValueTemplate
fn validate_references(
    template: &ValueTemplate,
    path: &[String],
    available_steps: &HashSet<String>,
    current_step_id: &str,
    diagnostics: &mut Diagnostics,
) {
    use stepflow_core::values::ValueTemplateRepr;
    use stepflow_core::workflow::Expr;

    match template.as_ref() {
        ValueTemplateRepr::Expression(expr) => {
            // Check if this is an EscapedLiteral - if so, don't validate its contents
            match expr {
                Expr::EscapedLiteral { .. } => {
                    // EscapedLiteral expressions are opaque - don't validate their internal structure
                    // against the outer flow's context
                }
                _ => {
                    validate_expression_references(
                        expr,
                        path,
                        available_steps,
                        current_step_id,
                        diagnostics,
                    );
                }
            }
        }
        ValueTemplateRepr::Object(obj) => {
            // Recursively validate object fields
            for (key, template) in obj {
                let mut field_path = path.to_vec();
                field_path.push(key.clone());
                validate_references(
                    template,
                    &field_path,
                    available_steps,
                    current_step_id,
                    diagnostics,
                );
            }
        }
        ValueTemplateRepr::Array(arr) => {
            // Validate each array element
            for (index, template) in arr.iter().enumerate() {
                let mut element_path = path.to_vec();
                element_path.push(index.to_string());
                validate_references(
                    template,
                    &element_path,
                    available_steps,
                    current_step_id,
                    diagnostics,
                );
            }
        }
        // Primitive values (Null, Bool, Number, String) don't contain references
        ValueTemplateRepr::Null
        | ValueTemplateRepr::Bool(_)
        | ValueTemplateRepr::Number(_)
        | ValueTemplateRepr::String(_) => {}
    }
}

/// Collect step dependencies from ValueTemplate
fn collect_step_dependencies(
    template: &ValueTemplate,
    dependencies: &mut HashSet<String>,
) -> Result<()> {
    // Extract dependencies using ValueTemplate's method
    let deps = crate::dependency::analyze_template_dependencies(template)?;

    for dep in deps.dependencies() {
        if let Some(step_id) = dep.step_id() {
            dependencies.insert(step_id.to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::DiagnosticMessage;
    use serde_json::json;
    use stepflow_core::workflow::{Component, ErrorAction, Flow, FlowV1, JsonPath, Step};

    fn create_test_step(id: &str, input: serde_json::Value) -> Step {
        Step {
            id: id.to_string(),
            component: Component::from_string("/mock/test"),
            input: serde_json::from_value(input).unwrap(),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_valid_workflow() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: Some("A test workflow".to_string()),
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![
                create_test_step("step1", json!({"$from": {"workflow": "input"}})),
                create_test_step("step2", json!({"$from": {"step": "step1"}})),
            ],
            output: ValueTemplate::step_ref("step2", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, _error, _warning) = diagnostics.counts();
        assert_eq!(fatal, 0, "Expected no fatal diagnostics");
    }

    #[test]
    fn test_forward_reference_error() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![
                create_test_step("step1", json!({"$from": {"step": "step2"}})), // Forward reference
                create_test_step("step2", json!({"$from": {"workflow": "input"}})),
            ],
            output: ValueTemplate::step_ref("step2", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, _error, _warning) = diagnostics.counts();
        assert!(fatal > 0, "Expected fatal diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::UndefinedStepReference { .. }))
        );
    }

    #[test]
    fn test_duplicate_step_ids() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![
                create_test_step("step1", json!({"$from": {"workflow": "input"}})),
                create_test_step("step1", json!({"$from": {"workflow": "input"}})), // Duplicate ID
            ],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, _error, _warning) = diagnostics.counts();
        assert!(fatal > 0, "Expected fatal diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::DuplicateStepId { .. }))
        );
    }

    #[test]
    fn test_self_reference() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![create_test_step(
                "step1",
                json!({"$from": {"step": "step1"}}),
            )],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, _error, _warning) = diagnostics.counts();
        assert!(fatal > 0, "Expected fatal diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::SelfReference { .. }))
        );
    }

    #[test]
    fn test_unreachable_step() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![
                create_test_step("step1", json!({"$from": {"workflow": "input"}})),
                create_test_step("step2", json!({"$from": {"workflow": "input"}})), // Not referenced
            ],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (_fatal, _error, warning) = diagnostics.counts();
        assert!(warning > 0, "Expected warnings");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::UnreachableStep { .. }))
        );
    }

    #[test]
    fn test_workflow_with_no_name_and_description() {
        let flow = Flow::V1(FlowV1 {
            name: None,        // Missing name
            description: None, // Missing description
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![create_test_step(
                "step1",
                json!({"$from": {"workflow": "input"}}),
            )],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, _error, warning) = diagnostics.counts();
        assert_eq!(fatal, 0, "Expected no fatal diagnostics"); // These are warnings, not fatal
        assert!(warning >= 2, "Expected at least 2 warnings"); // name + description + possibly others
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::MissingWorkflowName))
        );
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::MissingWorkflowDescription))
        );
    }

    #[test]
    fn test_empty_component_name() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![Step {
                id: "step1".to_string(),
                component: Component::from_string(""), // Empty builtin name
                input: ValueTemplate::workflow_input(JsonPath::default()),
                input_schema: None,
                output_schema: None,
                skip_if: None,
                on_error: ErrorAction::Fail,
                metadata: std::collections::HashMap::new(),
            }],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, error, _warning) = diagnostics.counts();
        assert_eq!(fatal, 0, "Expected no fatal diagnostics");
        assert!(error > 0, "Expected error diagnostics");
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| matches!(d.message, DiagnosticMessage::InvalidComponent { .. }))
        );
    }

    #[test]
    fn test_valid_builtin_component() {
        let flow = Flow::V1(FlowV1 {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![Step {
                id: "step1".to_string(),
                component: Component::from_string("/builtin/eval"), // Valid builtin
                input: ValueTemplate::workflow_input(JsonPath::default()),
                input_schema: None,
                output_schema: None,
                skip_if: None,
                on_error: ErrorAction::Fail,
                metadata: std::collections::HashMap::new(),
            }],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: None,
            metadata: std::collections::HashMap::new(),
        });

        let diagnostics = validate_workflow(&flow).unwrap();
        let (fatal, error, _warning) = diagnostics.counts();
        assert_eq!(fatal, 0, "Expected no fatal diagnostics");
        assert_eq!(error, 0, "Expected no error diagnostics for valid builtin");
        // Should have warnings but no errors for valid builtin components
    }
}
