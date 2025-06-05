use std::{collections::HashSet, sync::Arc};

use stepflow_core::workflow::{BaseRef, Expr, Flow, ValueRef};

use crate::{
    ExecutionError, Result,
    tracker::{Dependencies, DependenciesBuilder},
};

/// Build a dependency graph from a workflow.
pub fn build_dependencies_from_flow(flow: &Flow) -> Result<Arc<Dependencies>> {
    let mut builder = DependenciesBuilder::new(flow.steps.len());

    // Add each step and extract its dependencies
    for step in &flow.steps {
        let mut step_dependencies = HashSet::new();

        // Extract dependencies from step input
        collect_step_dependencies_from_value_ref(&step.input, &mut step_dependencies)?;

        // Extract dependencies from skip condition
        if let Some(skip_if) = &step.skip_if {
            collect_expr_dependencies(skip_if, &mut step_dependencies);
        }

        builder.add_step(&step.id, step_dependencies);
    }

    Ok(builder.finish())
}

/// Extract step dependencies from an expression.
/// Only collects dependencies on other steps, not workflow input.
///
/// This function uses match to handle all BaseRef variants, making it easy to extend
/// when new reference types are added to the BaseRef enum.
fn collect_expr_dependencies(expr: &Expr, step_deps: &mut HashSet<String>) {
    if let Some(base_ref) = expr.base_ref() {
        match base_ref {
            BaseRef::Step { step } => {
                step_deps.insert(step.clone());
            }
            BaseRef::Workflow(_) => {
                // Ignore workflow input dependencies - those are always available
            } // Future BaseRef variants can be easily added here
        }
    }
}

/// Collect step dependencies from a ValueRef.
/// Only collects dependencies on other steps, not workflow input.
fn collect_step_dependencies_from_value_ref(
    value_ref: &ValueRef,
    step_deps: &mut HashSet<String>,
) -> Result<()> {
    collect_step_dependencies(value_ref.as_ref(), step_deps)
}

/// Recursively collect step dependencies from a JSON value.
/// Only collects dependencies on other steps, not workflow input.
fn collect_step_dependencies(
    value: &serde_json::Value,
    step_deps: &mut HashSet<String>,
) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            // Check if this object is a literal wrapper
            if map.contains_key("$literal") {
                // Skip processing inside literal wrappers
                return Ok(());
            }

            // Check if this object is a reference
            if map.contains_key("$from") {
                // Try to deserialize this object as an Expr specifically as a Ref
                match serde_json::from_value::<Expr>(value.clone()) {
                    Ok(Expr::Ref { from, .. }) => {
                        // Successfully parsed as a reference expression
                        match from {
                            BaseRef::Step { step } => {
                                step_deps.insert(step);
                            }
                            BaseRef::Workflow(_) => {
                                // Ignore workflow input dependencies - those are always available
                            }
                        }
                        return Ok(()); // Don't recurse into reference objects
                    }
                    Ok(Expr::Literal(_)) => {
                        // If it parsed as a literal but has $from, it's malformed
                        return Err(ExecutionError::MalformedReference {
                            message: format!(
                                "Found object with '$from' key that was treated as literal instead of reference: {:?}",
                                value
                            )
                        }.into());
                    }
                    Err(e) => {
                        return Err(ExecutionError::MalformedReference {
                            message: format!(
                                "Found object with '$from' key that couldn't be parsed as expression: {:?}, error: {}",
                                value, e
                            )
                        }.into());
                    }
                }
            }

            // Not a reference or literal, recurse into all values
            for v in map.values() {
                collect_step_dependencies(v, step_deps)?;
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_step_dependencies(v, step_deps)?;
            }
        }
        _ => {
            // Primitive values cannot contain references
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_dependencies() {
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://test
    input:
      value: 42
  - id: step2
    component: mock://test
    input:
      message: hello
output:
  $from: {step: step2}
"#;

        let flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let deps = build_dependencies_from_flow(&flow).unwrap();
        let tracker = crate::tracker::DependencyTracker::new(deps);

        // Both steps should be runnable initially (no step dependencies)
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 2);
        assert!(unblocked.contains(0) && unblocked.contains(1));
    }

    #[test]
    fn test_simple_chain() {
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://test
    input:
      $from: {workflow: input}
  - id: step2
    component: mock://test
    input:
      $from: {step: step1}
  - id: step3
    component: mock://test
    input:
      $from: {step: step2}
output:
  $from: {step: step3}
"#;

        let flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let deps = build_dependencies_from_flow(&flow).unwrap();
        let mut tracker = crate::tracker::DependencyTracker::new(deps);

        // Only step1 should be runnable initially
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 1);
        assert!(unblocked.contains(0)); // step1

        // Complete step1 -> step2 becomes runnable
        let newly_unblocked = tracker.complete_step(0);
        assert_eq!(newly_unblocked.len(), 1);
        assert!(newly_unblocked.contains(1)); // step2

        // Complete step2 -> step3 becomes runnable
        let newly_unblocked = tracker.complete_step(1);
        assert_eq!(newly_unblocked.len(), 1);
        assert!(newly_unblocked.contains(2)); // step3
    }

    #[test]
    fn test_parallel_dependencies() {
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://test
    input:
      $from: {workflow: input}
  - id: step2
    component: mock://test
    input:
      $from: {workflow: input}
  - id: step3
    component: mock://test
    input:
      input1:
        $from: {step: step1}
      input2:
        $from: {step: step2}
output:
  $from: {step: step3}
"#;

        let flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let deps = build_dependencies_from_flow(&flow).unwrap();
        let mut tracker = crate::tracker::DependencyTracker::new(deps);

        // step1 and step2 should be runnable initially
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 2);
        assert!(unblocked.contains(0) && unblocked.contains(1));

        // Complete step1 -> step3 still blocked
        let newly_unblocked = tracker.complete_step(0);
        assert_eq!(newly_unblocked.len(), 0);

        // Complete step2 -> step3 becomes runnable
        let newly_unblocked = tracker.complete_step(1);
        assert_eq!(newly_unblocked.len(), 1);
        assert!(newly_unblocked.contains(2)); // step3
    }

    #[test]
    fn test_skip_if_dependency() {
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://test
    input:
      $from: {workflow: input}
  - id: step2
    component: mock://test
    input:
      $from: {workflow: input}
    skip_if:
      $from: {step: step1}
      path: should_skip
output:
  $from: {step: step2}
"#;

        let flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let deps = build_dependencies_from_flow(&flow).unwrap();
        let tracker = crate::tracker::DependencyTracker::new(deps);

        // Only step1 should be runnable initially (step2 depends on step1 for skip condition)
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 1);
        assert!(unblocked.contains(0)); // step1
    }

    #[test]
    fn test_nested_references() {
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://test
    input:
      $from: {workflow: input}
  - id: step2
    component: mock://test
    input:
      nested:
        deep:
          reference:
            $from: {step: step1}
      array:
        - $from: {step: step1}
        - literal: value
output:
  $from: {step: step2}
"#;

        let flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();
        let deps = build_dependencies_from_flow(&flow).unwrap();
        let tracker = crate::tracker::DependencyTracker::new(deps);

        // Only step1 should be runnable initially
        let unblocked = tracker.unblocked_steps();
        assert_eq!(unblocked.len(), 1);
        assert!(unblocked.contains(0)); // step1
    }

    #[test]
    fn test_malformed_reference_error() {
        // Test that malformed $from objects cause dependency analysis to fail
        // Note: This uses raw JSON since YAML can't represent the malformed reference structure
        let workflow_yaml = r#"
steps:
  - id: step1
    component: mock://test
    input:
      $from: {workflow: input}
  - id: step2
    component: mock://test
    input:
      valid_ref:
        $from: {step: step1}
      regular_field: normal_value
output:
  $from: {step: step2}
"#;

        let mut flow: Flow = serde_yaml_ng::from_str(workflow_yaml).unwrap();

        // Manually inject a malformed reference that can't be represented in YAML
        use serde_json::json;
        flow.steps[1].input = ValueRef::new(json!({
            "valid_ref": {"$from": {"step": "step1"}},
            "malformed_ref": {"$from": null, "invalid_field": "value"}, // This should trigger an error
            "regular_field": "normal_value"
        }));

        // This should now fail with malformed references instead of warning
        let deps_result = build_dependencies_from_flow(&flow);

        assert!(deps_result.is_err());

        // Verify the error message contains information about the malformed reference
        let error = deps_result.unwrap_err();
        assert!(error.to_string().contains("malformed reference"));
        assert!(error.to_string().contains("invalid_field"));
    }
}
