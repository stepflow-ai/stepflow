use std::collections::HashMap;
use stepflow_core::workflow::{BaseRef, Expr, Flow, ValueRef};

use crate::DestinationField;
use crate::{
    error::AnalysisError,
    types::{StepDependency, WorkflowAnalysis},
    Result,
};

/// Analyze a workflow for step dependencies
pub fn analyze_workflow_dependencies(
    flow: &Flow,
    workflow_hash: String,
) -> Result<WorkflowAnalysis> {
    let mut dependencies = Vec::new();

    // Create step ID to index mapping
    let step_id_to_index: HashMap<String, usize> = flow
        .steps
        .iter()
        .enumerate()
        .map(|(idx, step)| (step.id.clone(), idx))
        .collect();

    // Analyze each step for dependencies
    for (step_index, step) in flow.steps.iter().enumerate() {
        // Extract dependencies from step input
        let input_dependencies = extract_dependencies_from_value_ref(
            &step.input,
            &step_id_to_index,
            step_index,
            Some("input".to_string()),
        )?;
        dependencies.extend(input_dependencies);

        // Extract dependencies from skip condition
        if let Some(skip_if) = &step.skip_if {
            let skip_dependencies = extract_dependencies_from_expr(
                skip_if,
                &step_id_to_index,
                step_index,
                DestinationField::SkipIf,
            )?;
            dependencies.extend(skip_dependencies);
        }
    }

    Ok(WorkflowAnalysis::new(workflow_hash, dependencies, flow))
}

/// Extract dependencies from a ValueRef
fn extract_dependencies_from_value_ref(
    value_ref: &ValueRef,
    step_id_to_index: &HashMap<String, usize>,
    step_index: usize,
    dst_field: Option<String>,
) -> Result<Vec<StepDependency>> {
    extract_dependencies_from_value(value_ref.as_ref(), step_id_to_index, step_index, dst_field)
}

/// Extract dependencies from an expression
fn extract_dependencies_from_expr(
    expr: &Expr,
    step_id_to_index: &HashMap<String, usize>,
    step_index: usize,
    dst_field: DestinationField,
) -> Result<Vec<StepDependency>> {
    if let Expr::Ref { from, path, .. } = expr {
        if let BaseRef::Step { step } = from {
            let depends_on_step_index = *step_id_to_index.get(step).ok_or_else(|| {
                error_stack::report!(AnalysisError::StepNotFound {
                    step_id: step.clone(),
                })
            })?;

            return Ok(vec![StepDependency {
                step_index,
                depends_on_step_index,
                src_path: path.clone(),
                dst_field,
            }]);
        }
    }
    Ok(vec![])
}

/// Recursively extract dependencies from a JSON value
fn extract_dependencies_from_value(
    value: &serde_json::Value,
    step_id_to_index: &HashMap<String, usize>,
    step_index: usize,
    dst_field: Option<String>,
) -> Result<Vec<StepDependency>> {
    let mut dependencies = Vec::new();

    match value {
        serde_json::Value::Object(map) => {
            if map.contains_key("$literal") {
                // Skip processing inside literal wrappers
                return Ok(dependencies);
            } else if map.contains_key("$from") {
                // This is a reference object
                match serde_json::from_value::<Expr>(value.clone()) {
                    Ok(Expr::Ref { from, path, .. }) => {
                        // Successfully parsed as a reference expression
                        if let BaseRef::Step { step } = from {
                            let depends_on_step_index =
                                *step_id_to_index.get(&step).ok_or_else(|| {
                                    error_stack::report!(AnalysisError::StepNotFound {
                                        step_id: step.clone(),
                                    })
                                })?;

                            dependencies.push(StepDependency {
                                step_index,
                                depends_on_step_index,
                                src_path: path,
                                dst_field: DestinationField::from_input_field(dst_field),
                            });
                        }
                        // Don't recurse into reference objects
                        return Ok(dependencies);
                    }
                    Ok(Expr::Literal(_)) => {
                        // If it parsed as a literal but has $from, it's malformed
                        return Err(error_stack::report!(AnalysisError::MalformedReference {
                            message: format!(
                                "Found object with '$from' key that was treated as literal instead of reference: {:?}",
                                value
                            ),
                        }));
                    }
                    Err(e) => {
                        return Err(error_stack::report!(AnalysisError::MalformedReference {
                            message: format!(
                                "Found object with '$from' key that couldn't be parsed as expression: {:?}, error: {}",
                                value, e
                            ),
                        }));
                    }
                }
            } else {
                // Not a reference or literal, recurse into all values
                for (key, v) in map {
                    let dst_field = dst_field.clone().or_else(|| Some(key.to_owned()));
                    let nested_deps = extract_dependencies_from_value(
                        v,
                        step_id_to_index,
                        step_index,
                        dst_field.clone(),
                    )?;
                    dependencies.extend(nested_deps);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter() {
                let nested_deps = extract_dependencies_from_value(
                    v,
                    step_id_to_index,
                    step_index,
                    dst_field.clone(),
                )?;
                dependencies.extend(nested_deps);
            }
        }
        _ => {
            // Primitive values cannot contain references
        }
    }

    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::workflow::{Component, ErrorAction, Flow, Step};
    use url::Url;

    fn create_test_flow() -> Flow {
        Flow {
            name: Some("test_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![
                Step {
                    id: "step1".to_string(),
                    component: Component::new(Url::parse("mock://test").unwrap()),
                    input: ValueRef::new(json!({"$from": {"workflow": "input"}})),
                    input_schema: None,
                    output_schema: None,
                    skip_if: None,
                    on_error: ErrorAction::Fail,
                },
                Step {
                    id: "step2".to_string(),
                    component: Component::new(Url::parse("mock://test").unwrap()),
                    input: ValueRef::new(json!({"$from": {"step": "step1"}})),
                    input_schema: None,
                    output_schema: None,
                    skip_if: None,
                    on_error: ErrorAction::Fail,
                },
            ],
            output: json!({"$from": {"step": "step2"}}),
            test: None,
        }
    }

    #[test]
    fn test_analyze_simple_chain() {
        let flow = create_test_flow();
        let analysis = analyze_workflow_dependencies(&flow, "test_hash".to_string()).unwrap();

        assert_eq!(analysis.dependencies.len(), 1);
        let dep = &analysis.dependencies[0];
        assert_eq!(dep.step_index, 1); // step2
        assert_eq!(dep.depends_on_step_index, 0); // step1
    }

    #[test]
    fn test_analyze_with_skip_condition() {
        let mut flow = create_test_flow();
        // Add skip condition to step2 that depends on step1
        flow.steps[1].skip_if = Some(Expr::Ref {
            from: BaseRef::Step {
                step: "step1".to_string(),
            },
            path: Some("should_skip".to_string()),
            on_skip: stepflow_core::workflow::SkipAction::UseDefault {
                default_value: None,
            },
        });

        let analysis = analyze_workflow_dependencies(&flow, "test_hash".to_string()).unwrap();

        // Should find 2 dependencies: input dependency + skip condition dependency
        assert_eq!(analysis.dependencies.len(), 2);

        // Both should be from step2 to step1
        for dep in &analysis.dependencies {
            assert_eq!(dep.step_index, 1); // step2
            assert_eq!(dep.depends_on_step_index, 0); // step1
        }
    }

    #[test]
    fn test_step_not_found_error() {
        let mut flow = create_test_flow();
        // Reference a non-existent step
        flow.steps[1].input = ValueRef::new(json!({"$from": {"step": "nonexistent"}}));

        let result = analyze_workflow_dependencies(&flow, "test_hash".to_string());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Step not found: nonexistent"));
    }
}
