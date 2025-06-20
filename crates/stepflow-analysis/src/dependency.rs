use error_stack::ResultExt as _;
use indexmap::IndexMap;
use std::collections::HashSet;
use std::sync::Arc;
use stepflow_core::workflow::{BaseRef, Expr, Flow, FlowHash, Step, ValueRef, WorkflowRef};

use crate::{
    Result,
    error::AnalysisError,
    tracker::DependenciesBuilder,
    types::{Dependency, FlowAnalysis, StepAnalysis, ValueDependencies},
};

/// Analyze a workflow for step dependencies
pub fn analyze_workflow_dependencies(
    flow: Arc<Flow>,
    workflow_hash: FlowHash,
) -> Result<FlowAnalysis> {
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
    let output_depends = extract_value_deps(&flow.output)?;

    // Build dependency graph for execution
    let mut builder = DependenciesBuilder::new(steps.len());
    for (step_id, step_analysis) in &steps {
        let dependencies = extract_step_dependencies(step_analysis);
        builder.add_step(step_id, dependencies);
    }
    let dependencies = builder.finish();

    // TODO: Add validation logic
    let validation_errors = vec![];
    let validation_warnings = vec![];

    Ok(FlowAnalysis {
        flow_hash: workflow_hash,
        flow: flow.clone(),
        steps,
        output_depends,
        validation_errors,
        validation_warnings,
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
    // Extract dependencies from step input
    let input_depends = extract_value_deps(&step.input)?;

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

/// Extract dependencies from a ValueRef
fn extract_value_deps(value_ref: &ValueRef) -> Result<ValueDependencies> {
    match ParseResult::try_from(value_ref)? {
        ParseResult::Literal => Ok(ValueDependencies::Other(HashSet::new())),
        ParseResult::Expr(expr) => {
            let mut deps = HashSet::new();
            if let Some(dep) = extract_dep_from_expr(&expr)? {
                deps.insert(dep);
            } else {
                error_stack::bail!(AnalysisError::MalformedReference {
                    message: format!(
                        "Found object with '$from' key that parsed as a literal expression, which is invalid: {:?}",
                        value_ref
                    )
                });
            }
            Ok(ValueDependencies::Other(deps))
        }
        ParseResult::Value(serde_json::Value::Object(fields)) => {
            let fields = fields
                .iter()
                .map(|(f, v)| {
                    let mut deps = HashSet::new();
                    extract_deps_from_value(v, &mut deps)?;

                    Ok((f.to_owned(), deps))
                })
                .collect::<Result<IndexMap<_, _>>>()?;
            Ok(ValueDependencies::Object(fields))
        }
        ParseResult::Value(v) => {
            let mut deps = HashSet::new();
            extract_deps_from_value(v, &mut deps)?;
            Ok(ValueDependencies::Other(deps))
        }
    }
}

enum ParseResult<'a> {
    /// A literal. Dependencies inside the value should be ignored.
    Literal,
    /// An expression.
    Expr(Expr),
    /// A value. Dependencies inside the value should be extracted.
    Value(&'a serde_json::Value),
}

impl<'a> TryFrom<&'a ValueRef> for ParseResult<'a> {
    type Error = error_stack::Report<AnalysisError>;
    fn try_from(value: &'a ValueRef) -> Result<Self> {
        if let Some(fields) = value.as_object() {
            if fields.contains_key("$literal") {
                Ok(ParseResult::Literal)
            } else if fields.contains_key("$from") {
                let expr = serde_json::from_value::<Expr>(value.as_ref().clone()).change_context_lazy(||
                    AnalysisError::MalformedReference { message: format!(
                        "Found object with '$from' key that couldn't be parsed as expression: {:?}",
                        value
                    ) })?;
                Ok(ParseResult::Expr(expr))
            } else {
                Ok(ParseResult::Value(value.as_ref()))
            }
        } else {
            Ok(ParseResult::Value(value.as_ref()))
        }
    }
}

impl<'a> TryFrom<&'a serde_json::Value> for ParseResult<'a> {
    type Error = error_stack::Report<AnalysisError>;

    fn try_from(value: &'a serde_json::Value) -> Result<Self> {
        if let Some(fields) = value.as_object() {
            if fields.contains_key("$literal") {
                Ok(ParseResult::Literal)
            } else if fields.contains_key("$from") {
                let expr = serde_json::from_value::<Expr>(value.clone()).change_context_lazy(||
                    AnalysisError::MalformedReference { message: format!(
                        "Found object with '$from' key that couldn't be parsed as expression: {:?}",
                        value
                    ) })?;
                Ok(ParseResult::Expr(expr))
            } else {
                Ok(ParseResult::Value(value))
            }
        } else {
            Ok(ParseResult::Value(value))
        }
    }
}

/// Recursively extract dependencies from a JSON value
fn extract_deps_from_value(
    value: &serde_json::Value,
    deps: &mut HashSet<Dependency>,
) -> Result<()> {
    match ParseResult::try_from(value)? {
        ParseResult::Literal => return Ok(()),
        ParseResult::Expr(expr) => {
            if let Some(dep) = extract_dep_from_expr(&expr)? {
                deps.insert(dep);
            } else {
                error_stack::bail!(AnalysisError::MalformedReference {
                    message: format!(
                        "Found object with '$from' key that parsed as a literal expression, which is invalid: {:?}",
                        value
                    )
                });
            }
        }
        ParseResult::Value(serde_json::Value::Object(fields)) => {
            for v in fields.values() {
                extract_deps_from_value(v, deps)?;
            }
        }
        ParseResult::Value(serde_json::Value::Array(arr)) => {
            for v in arr.iter() {
                extract_deps_from_value(v, deps)?;
            }
        }
        ParseResult::Value(_) => {
            // Primitive values cannot contain references
        }
    }

    Ok(())
}

/// Extract dependencies from an expression
fn extract_dep_from_expr(expr: &Expr) -> Result<Option<Dependency>> {
    match expr {
        Expr::Ref {
            from,
            path,
            on_skip,
        } => {
            let field = path.clone();
            match from {
                BaseRef::Step { step } => Ok(Some(Dependency::StepOutput {
                    step_id: step.clone(),
                    field,
                    optional: on_skip.is_optional(),
                })),
                BaseRef::Workflow(WorkflowRef::Input) => Ok(Some(Dependency::FlowInput { field })),
            }
        }
        Expr::Literal(_) => Ok(None),
    }
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
            output: json!({"$from": {"step": "step2"}}).into(),
            test: None,
            examples: vec![],
        }
    }

    #[test]
    fn test_analyze_simple_chain() {
        let flow = create_test_flow();
        let analysis = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();

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
            path: Some("should_skip".to_string()),
            on_skip: stepflow_core::workflow::SkipAction::UseDefault {
                default_value: None,
            },
        });

        let analysis = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();

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
        flow.steps[1].input = ValueRef::new(json!({
            "data": {"$from": {"step": "step1"}},
            "config": {"$from": {"workflow": "input"}, "path": "config"},
            "literal_value": 42
        }));

        let analysis = analyze_workflow_dependencies(Arc::new(flow), "test_hash".into()).unwrap();

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
}
