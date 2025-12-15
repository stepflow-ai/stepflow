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

use crate::dependencies::Dependency;
use indexmap::IndexMap;
use std::sync::Arc;
use stepflow_core::{
    BlobId, ValueExpr,
    workflow::{BaseRef, Expr, Flow, Step, WorkflowRef},
};

use crate::{
    Result,
    tracker::DependenciesBuilder,
    types::{AnalysisResult, FlowAnalysis, StepAnalysis},
    validation::validate,
};

/// Analyze a workflow for step dependencies
/// Returns Ok(AnalysisResult) with analysis (if no fatal diagnostics) and all diagnostics
/// Returns Err(AnalysisError) for internal/unexpected errors
pub fn validate_and_analyze(flow: Arc<Flow>, flow_id: BlobId) -> Result<AnalysisResult> {
    // 1. Run validation first
    let diagnostics = validate(&flow)?;

    // 2. If fatal diagnostics exist, return diagnostics without analysis
    if diagnostics.has_fatal() {
        return Ok(AnalysisResult::with_diagnostics_only(diagnostics));
    }

    // 3. Proceed with dependency analysis (should not fail on user errors now)
    let analysis = analyze_dependencies_internal(flow.clone(), flow_id)?;

    // 4. Return analysis with diagnostics
    Ok(AnalysisResult::with_analysis(analysis, diagnostics))
}

/// Internal dependency analysis - assumes validation has already passed
fn analyze_dependencies_internal(flow: Arc<Flow>, flow_id: BlobId) -> Result<FlowAnalysis> {
    // Analyze each step for dependencies
    let steps = flow
        .steps()
        .iter()
        .map(|step| {
            let step_analysis = analyze_step(step)?;
            Ok((step.id.clone(), step_analysis))
        })
        .collect::<Result<IndexMap<_, _>>>()?;

    // Analyze workflow output dependencies
    let output_depends = analyze_template_dependencies(flow.output())?;

    // Build dependency graph for execution
    let mut builder = DependenciesBuilder::new(steps.len());
    for (step_id, step_analysis) in &steps {
        let dependencies = extract_step_dependencies(step_analysis);
        builder.add_step(step_id, dependencies);
    }
    let dependencies = builder.finish();

    Ok(FlowAnalysis {
        flow_id,
        flow,
        steps,
        output_depends,
        dependencies,
    })
}

/// Extract the list of step IDs that the given step depends on
fn extract_step_dependencies(step_analysis: &StepAnalysis) -> impl Iterator<Item = &str> + '_ {
    let dependencies = step_analysis.input_depends.step_dependencies();

    let skip_if = step_analysis
        .skip_if_depend
        .as_ref()
        .and_then(|s| s.step_id())
        .into_iter();

    dependencies.chain(skip_if)
}

fn analyze_step(step: &Step) -> Result<StepAnalysis> {
    let input_depends = analyze_template_dependencies(&step.input)?;

    // Extract dependencies from skip condition
    let skip_if_depend = if let Some(skip_if) = &step.skip_if {
        extract_dep_from_expr(skip_if)?
    } else {
        None
    };

    Ok(StepAnalysis {
        input_depends,
        skip_if_depend,
    })
}

/// Error types for dependency analysis
#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    #[error("Failed to parse expression")]
    ParseError,
    #[error("Malformed reference")]
    MalformedReference,
}

/// Analyze dependencies within a ValueExpr.
///
/// This handles both structured (object) and unstructured dependency analysis
/// by examining the expression structure recursively.
pub(crate) fn analyze_template_dependencies(
    expr: &ValueExpr,
) -> Result<crate::dependencies::ValueDependencies> {
    use crate::dependencies::ValueDependencies;

    match expr {
        ValueExpr::Object(fields) => {
            // For objects, analyze each field separately to maintain structure
            let field_deps = fields
                .iter()
                .map(|(field, field_expr)| {
                    let deps = collect_expr_dependencies(field_expr)?;
                    Ok((field.clone(), deps))
                })
                .collect::<Result<indexmap::IndexMap<_, _>>>()?;
            Ok(ValueDependencies::Object(field_deps))
        }
        _ => {
            // For non-objects, collect all dependencies into a single set
            let deps = collect_expr_dependencies(expr)?;
            Ok(ValueDependencies::Other(deps))
        }
    }
}

/// Recursively collect all step dependencies from a ValueExpr
fn collect_expr_dependencies(expr: &ValueExpr) -> Result<std::collections::HashSet<Dependency>> {
    use std::collections::HashSet;

    let mut deps = HashSet::new();

    match expr {
        ValueExpr::Step { step, .. } => {
            deps.insert(Dependency::StepOutput {
                step_id: step.clone(),
                field: None,
                optional: false,
            });
        }
        ValueExpr::Input { .. } => {
            deps.insert(Dependency::FlowInput { field: None });
        }
        ValueExpr::Variable { .. } => {
            // Variables are not step dependencies
        }
        ValueExpr::Array(items) => {
            for item in items {
                deps.extend(collect_expr_dependencies(item)?);
            }
        }
        ValueExpr::Object(fields) => {
            for (_key, value) in fields {
                deps.extend(collect_expr_dependencies(value)?);
            }
        }
        ValueExpr::Literal(_) | ValueExpr::EscapedLiteral { .. } => {
            // Literals have no dependencies
        }
    }

    Ok(deps)
}

/// Extract dependencies from an expression
fn extract_dep_from_expr(expr: &Expr) -> Result<Option<Dependency>> {
    match expr {
        Expr::Ref {
            from,
            path,
            on_skip,
        } => {
            let field = path.outer_field().map(|f| f.to_string());
            match from {
                BaseRef::Step { step } => Ok(Some(Dependency::StepOutput {
                    step_id: step.clone(),
                    field,
                    optional: on_skip.as_ref().is_some_and(|s| s.is_optional()),
                })),
                BaseRef::Workflow(WorkflowRef::Input) => Ok(Some(Dependency::FlowInput { field })),
                BaseRef::Variable { .. } => {
                    // Variables don't create step dependencies - they're resolved at runtime
                    Ok(None)
                }
            }
        }
        Expr::EscapedLiteral { .. } | Expr::Literal(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::dependencies::ValueDependencies;
    use crate::validation::make_path;
    use serde_json::json;
    use stepflow_core::workflow::{Flow, FlowBuilder, JsonPath, Step, StepBuilder};

    fn create_test_step(id: &str, input: serde_json::Value) -> Step {
        StepBuilder::mock_step(id).input_json(input).build()
    }

    fn create_test_flow() -> Flow {
        FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::Step {
                        step: "step1".to_string(),
                        path: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step2".to_string(),
                path: Default::default(),
            })
            .build()
    }

    #[test]
    fn test_analyze_simple_chain() {
        let flow = create_test_flow();
        let result = validate_and_analyze(
            Arc::new(flow),
            stepflow_core::BlobId::new(
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        let analysis = result.analysis.unwrap(); // Should succeed without validation errors

        // Should have 2 steps: step1 and step2
        assert_eq!(analysis.steps.len(), 2);

        // Check step2 has a dependency on step1
        let step2 = analysis.steps.get("step2").expect("Should find step2");
        let expected_step2_deps = ValueDependencies::Other({
            let mut deps = HashSet::new();
            deps.insert(Dependency::StepOutput {
                step_id: "step1".to_string(),
                field: None,
                optional: false,
            });
            deps
        });
        assert_eq!(step2.input_depends, expected_step2_deps);

        // Check step1 depends on workflow input
        let step1 = analysis.steps.get("step1").expect("Should find step1");
        let expected_step1_deps = ValueDependencies::Other({
            let mut deps = HashSet::new();
            deps.insert(Dependency::FlowInput { field: None });
            deps
        });
        assert_eq!(step1.input_depends, expected_step1_deps);
    }

    #[test]
    fn test_analyze_with_skip_condition() {
        let mut flow = create_test_flow();
        // Add skip condition to step2 that depends on step1
        flow.step_mut(1).skip_if = Some(Expr::Ref {
            from: BaseRef::Step {
                step: "step1".to_string(),
            },
            path: "should_skip".into(),
            on_skip: Some(stepflow_core::workflow::SkipAction::UseDefault {
                default_value: None,
            }),
        });

        let result = validate_and_analyze(
            Arc::new(flow),
            stepflow_core::BlobId::new(
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        let analysis = result.analysis.unwrap(); // Should succeed without validation errors

        // Should have 2 steps: step1 and step2
        assert_eq!(analysis.steps.len(), 2);

        // Check step2 has dependencies on step1 (input dependency)
        let step2 = analysis.steps.get("step2").expect("Should find step2");
        let expected_step2_deps = ValueDependencies::Other({
            let mut deps = HashSet::new();
            deps.insert(Dependency::StepOutput {
                step_id: "step1".to_string(),
                field: None,
                optional: false,
            });
            deps
        });
        assert_eq!(step2.input_depends, expected_step2_deps);

        // Check that step2 has a skip condition dependency
        let expected_skip_dep = Some(Dependency::StepOutput {
            step_id: "step1".to_string(),
            field: Some("should_skip".to_string()),
            optional: true,
        });
        assert_eq!(step2.skip_if_depend, expected_skip_dep);

        // Check step1 depends on workflow input
        let step1 = analysis.steps.get("step1").expect("Should find step1");
        let expected_step1_deps = ValueDependencies::Other({
            let mut deps = HashSet::new();
            deps.insert(Dependency::FlowInput { field: None });
            deps
        });
        assert_eq!(step1.input_depends, expected_step1_deps);
    }

    #[test]
    fn test_analyze_complex_input_object() {
        let mut flow = create_test_flow();
        // Give step2 a complex input object with multiple dependencies
        flow.step_mut(1).input = serde_json::from_value(json!({
            "data": {"$step": "step1"},
            "config": {"$input": "config"},
            "literal_value": 42
        }))
        .unwrap();

        let result = validate_and_analyze(
            Arc::new(flow),
            stepflow_core::BlobId::new(
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            )
            .unwrap(),
        )
        .unwrap();
        let analysis = result.analysis.unwrap(); // Should succeed without validation errors

        // step2 should have dependencies parsed as an object
        let step2 = analysis.steps.get("step2").expect("Should find step2");
        match &step2.input_depends {
            ValueDependencies::Object(fields) => {
                assert_eq!(fields.len(), 3);

                // Check "data" field depends on step1
                let data_deps = fields.get("data").expect("Should have data field");
                assert_eq!(data_deps.len(), 1);
                let data_dep = data_deps.iter().next().unwrap();
                assert!(
                    matches!(data_dep, Dependency::StepOutput { step_id, .. } if step_id == "step1")
                );

                // Check "config" field depends on workflow input
                let config_deps = fields.get("config").expect("Should have config field");
                assert_eq!(config_deps.len(), 1);
                let config_dep = config_deps.iter().next().unwrap();
                assert!(matches!(config_dep, Dependency::FlowInput { .. }));

                // Check "literal_value" field has no dependencies
                let literal_deps = fields
                    .get("literal_value")
                    .expect("Should have literal_value field");
                assert_eq!(literal_deps.len(), 0);
            }
            _ => panic!("Expected Object variant for step2 input"),
        }
    }

    #[test]
    fn test_validation_api_valid_workflow() {
        let flow = create_test_flow();
        let result = validate_and_analyze(
            Arc::new(flow),
            stepflow_core::BlobId::new(
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        // Valid workflow should have analysis
        assert!(result.has_analysis(), "Expected analysis to be present");
        assert!(
            !result.diagnostics.has_fatal(),
            "Expected no fatal diagnostics"
        );

        assert_eq!(
            result.diagnostics().num_fatal,
            0,
            "Expected no fatal diagnostics"
        );
        assert_eq!(
            result.diagnostics().num_error,
            0,
            "Expected no error diagnostics"
        );
        assert!(
            result.diagnostics().num_warning > 0,
            "Expected some warnings (mock components, field access)"
        );

        // Should be able to get the analysis
        let analysis = result.analysis.unwrap();
        assert_eq!(analysis.steps.len(), 2);
    }

    #[test]
    fn test_validation_api_invalid_workflow() {
        let flow = FlowBuilder::new()
            .name("invalid_workflow")
            .steps(vec![
                create_test_step("step1", json!({"$step": "step2"})), // Forward reference
                create_test_step("step1", json!({"$input": "$"})), // Duplicate ID
            ])
            .output(ValueExpr::Step {
                step: "step1".to_string(),
                path: Default::default(),
            })
            .build();

        let result = validate_and_analyze(
            Arc::new(flow),
            stepflow_core::BlobId::new(
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
            )
            .unwrap(),
        )
        .unwrap();

        // Invalid workflow should not have analysis
        assert!(
            !result.has_analysis(),
            "Expected no analysis due to fatal diagnostics"
        );

        assert!(
            result.diagnostics().num_fatal > 0,
            "Expected fatal diagnostics"
        );

        // Analysis should be None
        assert!(result.analysis.is_none(), "Expected no analysis");

        // Should have specific diagnostic messages
        let has_duplicate_id = result.diagnostics.diagnostics.iter().any(|d| {
            matches!(
                d.message(),
                crate::diagnostics::DiagnosticMessage::DuplicateStepId { .. }
            )
        });
        let has_undefined_reference = result.diagnostics.diagnostics.iter().any(|d| {
            matches!(
                d.message(),
                crate::diagnostics::DiagnosticMessage::UndefinedStepReference { .. }
            )
        });

        assert!(has_duplicate_id, "Expected duplicate step ID diagnostic");
        assert!(
            has_undefined_reference,
            "Expected undefined step reference diagnostic but was: {:?}",
            result.diagnostics,
        );
    }

    #[test]
    fn test_analysis_result_convenience_methods() {
        use crate::diagnostics::{DiagnosticMessage, Diagnostics};

        // Test with analysis
        let flow = create_test_flow();
        let test_blob_id = BlobId::new("a".repeat(64)).unwrap();
        let analysis_result = analyze_dependencies_internal(Arc::new(flow), test_blob_id).unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.add(
            DiagnosticMessage::MockComponent {
                step_id: "test".to_string(),
            },
            make_path!(),
        );

        let result = crate::types::AnalysisResult::with_analysis(analysis_result, diagnostics);
        assert!(result.has_analysis());
        assert_eq!(result.diagnostics().num_fatal, 0);
        assert_eq!(result.diagnostics().num_error, 0);
        assert_eq!(result.diagnostics().num_warning, 1);

        // Test without analysis
        let mut diagnostics = Diagnostics::new();
        diagnostics.add(
            DiagnosticMessage::DuplicateStepId {
                step_id: "test".to_string(),
            },
            make_path!(),
        );

        let result = crate::types::AnalysisResult::with_diagnostics_only(diagnostics);
        assert!(!result.has_analysis());
        assert_eq!(result.diagnostics().num_fatal, 1);
        assert_eq!(result.diagnostics().num_error, 0);
        assert_eq!(result.diagnostics().num_warning, 0);
    }

    #[test]
    fn test_must_execute_steps_are_tracked() {
        // Create a flow where step3 has must_execute but is not referenced by output
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::Step {
                        step: "step1".to_string(),
                        path: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .must_execute(true) // Mark as must_execute
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step2".to_string(),
                path: Default::default(),
            })
            .build();

        let result =
            validate_and_analyze(Arc::new(flow), BlobId::new("a".repeat(64)).unwrap()).unwrap();

        let analysis = result.analysis.expect("Analysis should succeed");

        // Create a tracker to verify required steps tracking
        let mut tracker = analysis.new_dependency_tracker();

        // Initially step1 and step3 should be runnable (no dependencies)
        let unblocked = tracker.unblocked_steps();
        assert!(unblocked.contains(0), "step1 should be initially runnable");
        assert!(unblocked.contains(2), "step3 should be initially runnable");
        assert!(!unblocked.contains(1), "step2 should be blocked on step1");

        // Required steps (step2 for output + step3 for must_execute) are NOT completed yet
        assert!(
            !tracker.all_required_completed(),
            "step2 and step3 (required) have not completed yet"
        );

        // Complete step1
        let newly_unblocked = tracker.complete_step(0);
        assert!(
            newly_unblocked.contains(1),
            "step2 should be unblocked after step1"
        );

        // Still not all required steps completed
        assert!(
            !tracker.all_required_completed(),
            "step2 and step3 (required) have not completed yet"
        );

        // Complete step2 (output dependency satisfied, but step3 still required)
        tracker.complete_step(1);

        // Still not all required steps completed (step3 must_execute still needed)
        assert!(
            !tracker.all_required_completed(),
            "step3 (must_execute) has not completed yet"
        );

        // Complete step3 (must_execute step)
        tracker.complete_step(2);

        // Now all required steps are completed (output deps + must_execute)
        assert!(
            tracker.all_required_completed(),
            "All required steps (step2 for output, step3 for must_execute) have completed"
        );
    }

    #[test]
    fn test_only_required_steps_execute() {
        // Create a flow with an unreferenced step that should NOT execute
        // step1 -> step2 (output depends on step2)
        // step3 (not referenced, not must_execute, should NOT run)
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::Step {
                        step: "step1".to_string(),
                        path: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step2".to_string(),
                path: Default::default(),
            })
            .build();

        let result =
            validate_and_analyze(Arc::new(flow), BlobId::new("a".repeat(64)).unwrap()).unwrap();

        let analysis = result.analysis.expect("Analysis should succeed");

        // Create a tracker to verify only required steps are runnable
        let mut tracker = analysis.new_dependency_tracker();

        // Initially only step1 should be runnable (step3 is not required)
        let unblocked = tracker.unblocked_steps();
        assert!(unblocked.contains(0), "step1 should be runnable");
        assert!(
            !unblocked.contains(2),
            "step3 should NOT be runnable (not required)"
        );

        // Complete step1
        let newly_unblocked = tracker.complete_step(0);
        assert!(
            newly_unblocked.contains(1),
            "step2 should be newly runnable"
        );
        assert!(
            !newly_unblocked.contains(2),
            "step3 should NOT be newly runnable (not required)"
        );

        // Complete step2
        tracker.complete_step(1);

        // Now all required steps are completed
        assert!(
            tracker.all_required_completed(),
            "All required steps completed"
        );

        // Verify step3 never became runnable
        let unblocked = tracker.unblocked_steps();
        assert!(
            !unblocked.contains(2),
            "step3 should never be runnable (not required)"
        );
    }

    #[test]
    fn test_must_execute_without_dependencies() {
        // Create a flow where step2 has must_execute but no dependencies and isn't referenced
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::literal(json!({"value": 42})))
                    .must_execute(true)
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step1".to_string(),
                path: Default::default(),
            })
            .build();

        let result =
            validate_and_analyze(Arc::new(flow), BlobId::new("a".repeat(64)).unwrap()).unwrap();

        let analysis = result.analysis.expect("Analysis should succeed");

        // Create a tracker
        let mut tracker = analysis.new_dependency_tracker();

        // Both step1 and step2 should be initially runnable
        let unblocked = tracker.unblocked_steps();
        assert!(unblocked.contains(0), "step1 should be runnable");
        assert!(unblocked.contains(1), "step2 should be runnable");

        // Required steps (step1 for output + step2 for must_execute) not completed yet
        assert!(
            !tracker.all_required_completed(),
            "step1 and step2 (required) have not completed yet"
        );

        // Complete step1 (satisfies output dependency, but step2 still required)
        tracker.complete_step(0);

        // Required step2 (must_execute) still not completed
        assert!(
            !tracker.all_required_completed(),
            "step2 (must_execute) has not completed yet"
        );

        // Complete step2
        tracker.complete_step(1);

        // Now all required steps are completed (output deps + must_execute)
        assert!(
            tracker.all_required_completed(),
            "All required steps (step1 for output, step2 for must_execute) have completed"
        );
    }

    #[test]
    fn test_must_execute_with_skip_if_requires_condition_inputs() {
        // Test case 1: If a step is must_execute and has skip_if, then the inputs to
        // the skip_if condition are required
        //
        // step1 (provides input for skip_if)
        // step2 (must_execute with skip_if referencing step1, not used in output)
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::literal(json!({"value": 42})))
                    .skip_if(Expr::Ref {
                        from: BaseRef::Step {
                            step: "step1".to_string(),
                        },
                        path: JsonPath::default(),
                        on_skip: None,
                    })
                    .must_execute(true)
                    .build(),
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step3".to_string(),
                path: Default::default(),
            })
            .build();

        let result =
            validate_and_analyze(Arc::new(flow), BlobId::new("a".repeat(64)).unwrap()).unwrap();

        let analysis = result.analysis.expect("Analysis should succeed");

        // Verify that step1 is in required steps (needed for step2's skip_if)
        let mut tracker = analysis.new_dependency_tracker();

        // Initially step1 and step3 should be runnable
        let unblocked = tracker.unblocked_steps();
        assert!(
            unblocked.contains(0),
            "step1 should be runnable (required for skip_if)"
        );
        assert!(
            !unblocked.contains(1),
            "step2 should be blocked on step1 (for skip_if)"
        );
        assert!(
            unblocked.contains(2),
            "step3 should be runnable (output dependency)"
        );

        // Complete step1 - should unblock step2
        let newly_unblocked = tracker.complete_step(0);
        assert!(
            newly_unblocked.contains(1),
            "step2 should be unblocked after step1"
        );

        // Complete step2 (must_execute)
        tracker.complete_step(1);

        // Complete step3 (output)
        tracker.complete_step(2);

        // All required steps completed
        assert!(
            tracker.all_required_completed(),
            "All required steps completed"
        );
    }

    #[test]
    fn test_must_execute_skip_if_false_requires_step_inputs() {
        // Test case 2: If a step is must_execute and skip_if evaluates to false,
        // then the step's inputs are required
        //
        // Note: This test verifies dependency analysis considers inputs of must_execute
        // steps as required. Runtime evaluation of skip_if is handled by the executor.
        //
        // step1 (provides input for step2)
        // step2 (must_execute with input from step1, skip_if will be false at runtime)
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::Step {
                        step: "step1".to_string(),
                        path: Default::default(),
                    })
                    .skip_if(Expr::Ref {
                        from: BaseRef::Workflow(WorkflowRef::Input),
                        path: JsonPath::parse("$.should_skip").unwrap(),
                        on_skip: None,
                    })
                    .must_execute(true)
                    .build(),
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step3".to_string(),
                path: Default::default(),
            })
            .build();

        let result =
            validate_and_analyze(Arc::new(flow), BlobId::new("a".repeat(64)).unwrap()).unwrap();

        let analysis = result.analysis.expect("Analysis should succeed");

        // Verify that step1 is in required steps (needed for step2's input)
        let mut tracker = analysis.new_dependency_tracker();

        // Initially step1 and step3 should be runnable
        let unblocked = tracker.unblocked_steps();
        assert!(
            unblocked.contains(0),
            "step1 should be runnable (required for step2 input)"
        );
        assert!(!unblocked.contains(1), "step2 should be blocked on step1");
        assert!(
            unblocked.contains(2),
            "step3 should be runnable (output dependency)"
        );

        // Complete step1 - should unblock step2
        let newly_unblocked = tracker.complete_step(0);
        assert!(
            newly_unblocked.contains(1),
            "step2 should be unblocked after step1"
        );
    }

    #[test]
    fn test_must_execute_skip_if_true_does_not_require_step_inputs() {
        // Test case 3: If a step is must_execute but skip_if evaluates to true,
        // the step won't execute, but we still mark it as required during analysis
        // (since we can't know at analysis time whether skip_if will be true)
        //
        // Note: At analysis time, we conservatively include must_execute steps
        // and their dependencies in required_steps. The executor handles skipping
        // at runtime based on skip_if evaluation.
        //
        // step1 (provides input for step2)
        // step2 (must_execute with skip_if that references workflow input)
        // step3 (output)
        let flow = FlowBuilder::test_flow()
            .steps(vec![
                StepBuilder::mock_step("step1")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
                StepBuilder::mock_step("step2")
                    .input(ValueExpr::Step {
                        step: "step1".to_string(),
                        path: Default::default(),
                    })
                    .skip_if(Expr::Ref {
                        from: BaseRef::Workflow(WorkflowRef::Input),
                        path: JsonPath::parse("$.should_skip").unwrap(),
                        on_skip: None,
                    })
                    .must_execute(true)
                    .build(),
                StepBuilder::mock_step("step3")
                    .input(ValueExpr::Input {
                        input: Default::default(),
                    })
                    .build(),
            ])
            .output(ValueExpr::Step {
                step: "step3".to_string(),
                path: Default::default(),
            })
            .build();

        let result =
            validate_and_analyze(Arc::new(flow), BlobId::new("a".repeat(64)).unwrap()).unwrap();

        let analysis = result.analysis.expect("Analysis should succeed");

        // At analysis time, step1 is required (we don't evaluate skip_if statically)
        let tracker = analysis.new_dependency_tracker();

        // Initially step1 and step3 should be runnable
        let unblocked = tracker.unblocked_steps();
        assert!(
            unblocked.contains(0),
            "step1 should be runnable (conservatively required for step2)"
        );
        assert!(!unblocked.contains(1), "step2 should be blocked on step1");
        assert!(
            unblocked.contains(2),
            "step3 should be runnable (output dependency)"
        );

        // This test validates that analysis conservatively includes dependencies.
        // Actual skip_if evaluation happens at runtime in the executor.
    }
}
