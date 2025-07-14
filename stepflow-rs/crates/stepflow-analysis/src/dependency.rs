// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use crate::dependencies::Dependency;
use indexmap::IndexMap;
use std::sync::Arc;
use stepflow_core::workflow::{BaseRef, Expr, Flow, FlowHash, Step, ValueTemplate, WorkflowRef};

use crate::{
    Result,
    tracker::DependenciesBuilder,
    types::{AnalysisResult, FlowAnalysis, StepAnalysis},
    validation::validate_workflow,
};

/// Analyze a workflow for step dependencies
/// Returns Ok(AnalysisResult) with analysis (if no fatal diagnostics) and all diagnostics
/// Returns Err(AnalysisError) for internal/unexpected errors
pub fn analyze_workflow_dependencies(
    flow: Arc<Flow>,
    workflow_hash: FlowHash,
) -> Result<AnalysisResult> {
    // 1. Run validation first
    let diagnostics = validate_workflow(&flow)?;

    // 2. If fatal diagnostics exist, return diagnostics without analysis
    if diagnostics.has_fatal() {
        return Ok(AnalysisResult::with_diagnostics_only(diagnostics));
    }

    // 3. Proceed with dependency analysis (should not fail on user errors now)
    let analysis = analyze_dependencies_internal(flow.clone(), workflow_hash)?;

    // 4. Return analysis with diagnostics
    Ok(AnalysisResult::with_analysis(analysis, diagnostics))
}

/// Validate a workflow without performing full analysis
/// Returns Ok(AnalysisResult) with diagnostics but no analysis
/// Returns Err(AnalysisError) for internal/unexpected errors
pub fn validate_workflow_only(flow: &Flow) -> Result<AnalysisResult> {
    // Run validation
    let diagnostics = validate_workflow(flow)?;

    // Return diagnostics without analysis
    Ok(AnalysisResult::with_diagnostics_only(diagnostics))
}

/// Internal dependency analysis - assumes validation has already passed
fn analyze_dependencies_internal(flow: Arc<Flow>, workflow_hash: FlowHash) -> Result<FlowAnalysis> {
    // Analyze each step for dependencies
    let steps = flow
        .steps
        .iter()
        .map(|step| {
            let step_analysis = analyze_step(step)?;
            Ok((step.id.clone(), step_analysis))
        })
        .collect::<Result<IndexMap<_, _>>>()?;

    // Analyze workflow output dependencies
    let output_depends = analyze_template_dependencies(&flow.output)?;

    // Build dependency graph for execution
    let mut builder = DependenciesBuilder::new(steps.len());
    for (step_id, step_analysis) in &steps {
        let dependencies = extract_step_dependencies(step_analysis);
        builder.add_step(step_id, dependencies);
    }
    let dependencies = builder.finish();

    Ok(FlowAnalysis {
        flow_hash: workflow_hash,
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

/// Analyze dependencies within a ValueTemplate using the expression iterator.
///
/// This handles both structured (object) and unstructured dependency analysis
/// by examining the template structure and iterating over contained expressions.
pub(crate) fn analyze_template_dependencies(
    template: &ValueTemplate,
) -> Result<crate::dependencies::ValueDependencies> {
    use crate::dependencies::ValueDependencies;
    use std::collections::HashSet;
    use stepflow_core::values::ValueTemplateRepr;

    match template.as_ref() {
        ValueTemplateRepr::Object(obj) => {
            // For objects, analyze each field separately to maintain structure
            let fields = obj
                .iter()
                .map(|(field, field_template)| {
                    let mut deps = HashSet::new();
                    for expr in field_template.expressions() {
                        if let Some(dep) = extract_dep_from_expr(expr)? {
                            deps.insert(dep);
                        }
                    }
                    Ok((field.clone(), deps))
                })
                .collect::<Result<indexmap::IndexMap<_, _>>>()?;
            Ok(ValueDependencies::Object(fields))
        }
        _ => {
            // For non-objects, collect all dependencies into a single set
            let mut deps = HashSet::new();
            for expr in template.expressions() {
                if let Some(dep) = extract_dep_from_expr(expr)? {
                    deps.insert(dep);
                }
            }
            Ok(ValueDependencies::Other(deps))
        }
    }
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
                    optional: on_skip.is_optional(),
                })),
                BaseRef::Workflow(WorkflowRef::Input) => Ok(Some(Dependency::FlowInput { field })),
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
    use serde_json::json;
    use stepflow_core::workflow::{Component, ErrorAction, Flow, JsonPath, Step};

    fn create_test_step(id: &str, input: serde_json::Value) -> Step {
        Step {
            id: id.to_string(),
            component: Component::from_string("/mock/test"),
            input: ValueTemplate::parse_value(input).unwrap(),
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: ErrorAction::Fail,
        }
    }

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
                    component: Component::from_string("/mock/test"),
                    input: ValueTemplate::workflow_input(JsonPath::default()),
                    input_schema: None,
                    output_schema: None,
                    skip_if: None,
                    on_error: ErrorAction::Fail,
                },
                Step {
                    id: "step2".to_string(),
                    component: Component::from_string("/mock/test"),
                    input: ValueTemplate::step_ref("step1", JsonPath::default()),
                    input_schema: None,
                    output_schema: None,
                    skip_if: None,
                    on_error: ErrorAction::Fail,
                },
            ],
            output: ValueTemplate::step_ref("step2", JsonPath::default()),
            test: None,
            examples: vec![],
        }
    }

    #[test]
    fn test_analyze_simple_chain() {
        let flow = create_test_flow();
        let result = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();
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
        flow.steps[1].skip_if = Some(Expr::Ref {
            from: BaseRef::Step {
                step: "step1".to_string(),
            },
            path: "should_skip".into(),
            on_skip: stepflow_core::workflow::SkipAction::UseDefault {
                default_value: None,
            },
        });

        let result = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();
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
        flow.steps[1].input = ValueTemplate::parse_value(json!({
            "data": {"$from": {"step": "step1"}},
            "config": {"$from": {"workflow": "input"}, "path": "config"},
            "literal_value": 42
        }))
        .unwrap();

        let result = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();
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
    fn test_new_validation_api_valid_workflow() {
        let flow = create_test_flow();
        let result = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();

        // Valid workflow should have analysis
        assert!(result.has_analysis(), "Expected analysis to be present");
        assert!(
            !result.has_fatal_diagnostics(),
            "Expected no fatal diagnostics"
        );

        let (fatal, error, warning) = result.diagnostic_counts();
        assert_eq!(fatal, 0, "Expected no fatal diagnostics");
        assert_eq!(error, 0, "Expected no error diagnostics");
        assert!(
            warning > 0,
            "Expected some warnings (mock components, field access)"
        );

        // Should be able to get the analysis
        let analysis = result.analysis.unwrap();
        assert_eq!(analysis.steps.len(), 2);
    }

    #[test]
    fn test_new_validation_api_invalid_workflow() {
        let flow = Flow {
            name: Some("invalid_workflow".to_string()),
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![
                create_test_step("step1", json!({"$from": {"step": "step2"}})), // Forward reference
                create_test_step("step1", json!({"$from": {"workflow": "input"}})), // Duplicate ID
            ],
            output: ValueTemplate::step_ref("step1", JsonPath::default()),
            test: None,
            examples: vec![],
        };

        let result = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();

        // Invalid workflow should not have analysis
        assert!(
            !result.has_analysis(),
            "Expected no analysis due to fatal diagnostics"
        );
        assert!(result.has_fatal_diagnostics(), "Expected fatal diagnostics");

        let (fatal, _error, _warning) = result.diagnostic_counts();
        assert!(fatal > 0, "Expected fatal diagnostics");

        // Analysis should be None
        assert!(result.analysis.is_none(), "Expected no analysis");

        // Should have specific diagnostic messages
        let has_duplicate_id = result.diagnostics.diagnostics.iter().any(|d| {
            matches!(
                d.message,
                crate::diagnostics::DiagnosticMessage::DuplicateStepId { .. }
            )
        });
        let has_undefined_reference = result.diagnostics.diagnostics.iter().any(|d| {
            matches!(
                d.message,
                crate::diagnostics::DiagnosticMessage::UndefinedStepReference { .. }
            )
        });

        assert!(has_duplicate_id, "Expected duplicate step ID diagnostic");
        assert!(
            has_undefined_reference,
            "Expected undefined step reference diagnostic"
        );
    }

    #[test]
    fn test_diagnostic_levels_behavior() {
        use crate::diagnostics::{Diagnostic, DiagnosticLevel, DiagnosticMessage};

        // Test that fatal diagnostics block analysis
        let fatal = Diagnostic::new(DiagnosticMessage::DuplicateStepId {
            step_id: "test".to_string(),
        });
        assert_eq!(fatal.level, DiagnosticLevel::Fatal);
        assert!(fatal.blocks_analysis());
        assert!(fatal.indicates_execution_failure());

        // Test that errors don't block analysis but indicate execution failure
        let error = Diagnostic::new(DiagnosticMessage::InvalidFieldAccess {
            step_id: "test".to_string(),
            field: "field".to_string(),
            reason: "missing".to_string(),
        });
        assert_eq!(error.level, DiagnosticLevel::Error);
        assert!(!error.blocks_analysis());
        assert!(error.indicates_execution_failure());

        // Test that warnings don't block analysis or indicate execution failure
        let warning = Diagnostic::new(DiagnosticMessage::MockComponent {
            step_id: "test".to_string(),
        });
        assert_eq!(warning.level, DiagnosticLevel::Warning);
        assert!(!warning.blocks_analysis());
        assert!(!warning.indicates_execution_failure());
    }

    #[test]
    fn test_analysis_result_convenience_methods() {
        use crate::diagnostics::{DiagnosticMessage, Diagnostics};

        // Test with analysis
        let flow = create_test_flow();
        let analysis_result = analyze_dependencies_internal(Arc::new(flow), "test".into()).unwrap();
        let mut diagnostics = Diagnostics::new();
        diagnostics.add(
            DiagnosticMessage::MockComponent {
                step_id: "test".to_string(),
            },
            vec![],
        );

        let result = crate::types::AnalysisResult::with_analysis(analysis_result, diagnostics);
        assert!(result.has_analysis());
        assert!(!result.has_fatal_diagnostics());
        let (fatal, error, warning) = result.diagnostic_counts();
        assert_eq!(fatal, 0);
        assert_eq!(error, 0);
        assert_eq!(warning, 1);

        // Test without analysis
        let mut diagnostics = Diagnostics::new();
        diagnostics.add(
            DiagnosticMessage::DuplicateStepId {
                step_id: "test".to_string(),
            },
            vec![],
        );

        let result = crate::types::AnalysisResult::with_diagnostics_only(diagnostics);
        assert!(!result.has_analysis());
        assert!(result.has_fatal_diagnostics());
        let (fatal, error, warning) = result.diagnostic_counts();
        assert_eq!(fatal, 1);
        assert_eq!(error, 0);
        assert_eq!(warning, 0);
    }
}
