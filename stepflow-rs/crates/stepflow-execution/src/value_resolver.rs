use std::{collections::HashMap, sync::Arc};

use error_stack::ResultExt as _;
use stepflow_core::{
    FlowResult,
    workflow::{BaseRef, Expr, SkipAction, ValueRef},
};
use stepflow_state::StateStore;
use uuid::Uuid;

use crate::{ExecutionError, Result, write_cache::WriteCache};
use stepflow_core::workflow::Flow;
use stepflow_core::workflow::StepId;

/// Value resolver for handling expression and JSON value resolution
pub struct ValueResolver {
    /// Execution ID of the workflow we are resolving for.
    ///
    /// This is used to scope the state store interactions.
    execution_id: Uuid,
    /// Input value for the workflow.
    ///
    /// This is used to resolve references to the workflow input.
    input: ValueRef,
    /// State store to use for resolving values.
    ///
    /// This is used to store and retrieve step results.
    state_store: Arc<dyn StateStore>,
    /// Write cache for checking recently completed step results.
    ///
    /// This is checked before querying the state store for step results.
    write_cache: WriteCache,
    /// Map from step ID to step index for cache lookups.
    step_id_to_index: HashMap<String, usize>,
    flow: Arc<Flow>,
}

impl ValueResolver {
    pub fn new(
        execution_id: Uuid,
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        write_cache: WriteCache,
        flow: Arc<Flow>,
    ) -> Self {
        let step_id_to_index = flow
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (step.id.clone(), index))
            .collect();
        Self {
            execution_id,
            input,
            state_store,
            write_cache,
            step_id_to_index,
            flow,
        }
    }

    /// Check if we have cached results for all step IDs
    pub async fn has_cached_step_ids(&self, step_ids: &[String]) -> bool {
        for step_id in step_ids {
            if let Ok(step_id) = self.get_step_id(step_id) {
                if self.write_cache.get_step_result(&step_id).await.is_none() {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Resolve a ValueRef, returning a FlowResult.
    /// This is the main entry point for value resolution.
    pub async fn resolve(&self, value: &ValueRef) -> Result<FlowResult> {
        self.resolve_rec(value.as_ref()).await
    }

    fn get_step_id(&self, step_id: &str) -> Result<StepId> {
        if let Some(&step_index) = self.step_id_to_index.get(step_id) {
            Ok(StepId {
                index: step_index,
                flow: self.flow.clone(),
            })
        } else {
            Err(ExecutionError::UndefinedValue(BaseRef::Step {
                step: step_id.to_string(),
            })
            .into())
        }
    }

    pub async fn resolve_step(&self, step: &str) -> Result<FlowResult> {
        let step_id = self.get_step_id(step)?;
        self.write_cache
            .get_step_result_with_fallback(step_id, self.execution_id, &self.state_store)
            .await
            .change_context(ExecutionError::StateError)
    }

    /// Resolve an expression, returning a FlowResult.
    pub async fn resolve_expr(&self, expr: &Expr) -> Result<FlowResult> {
        // Handle literal expressions
        if let Expr::Literal(value) = expr {
            return Ok(FlowResult::Success {
                result: value.clone(),
            });
        }

        // Get the base reference
        let base_ref = expr.base_ref().ok_or(ExecutionError::Internal)?;

        let base_result = match base_ref {
            BaseRef::Workflow(_) => {
                // Return the workflow input
                FlowResult::Success {
                    result: self.input.clone(),
                }
            }
            BaseRef::Step { step: step_id } => self.resolve_step(step_id).await?,
        };

        // Apply path if specified
        let path_result = if let Some(path) = expr.path() {
            match base_result {
                FlowResult::Success { result } => {
                    if let Some(sub_value) = result.path(path) {
                        FlowResult::Success { result: sub_value }
                    } else {
                        return Err(ExecutionError::UndefinedField {
                            field: path.to_string(),
                            value: result,
                        }
                        .into());
                    }
                }
                FlowResult::Skipped => FlowResult::Skipped,
                other => other,
            }
        } else {
            base_result
        };

        // Handle skip actions.
        // NOTE: Skip actions are applied after path resolution.
        match path_result {
            FlowResult::Success { result } => Ok(FlowResult::Success { result }),
            FlowResult::Skipped => {
                match expr.on_skip() {
                    Some(SkipAction::UseDefault { default_value }) => {
                        let default = default_value
                            .as_ref()
                            .map(|v| v.as_ref())
                            .unwrap_or(&serde_json::Value::Null);
                        Ok(FlowResult::Success {
                            result: ValueRef::new(default.clone()),
                        })
                    }
                    _ => {
                        // No on_skip action specified - propagate the skip
                        Ok(FlowResult::Skipped)
                    }
                }
            }
            FlowResult::Failed { error } => Ok(FlowResult::Failed { error }),
        }
    }

    /// Recursive resolution of JSON values, returning FlowResult.
    async fn resolve_rec(&self, value: &serde_json::Value) -> Result<FlowResult> {
        match value {
            serde_json::Value::Object(fields) => {
                // Try to parse as an expression first
                if let Ok(expr) = serde_json::from_value::<Expr>(value.clone()) {
                    match expr {
                        Expr::Ref { .. } => {
                            // It's a reference - resolve it
                            return self.resolve_expr(&expr).await;
                        }
                        Expr::Literal(_) => {
                            // Check if this is actually a $literal wrapper (has $literal key)
                            if let Some(literal_content) = fields.get("$literal") {
                                // It's a literal wrapper - return the unwrapped value
                                return Ok(FlowResult::Success {
                                    result: ValueRef::new(literal_content.clone()),
                                });
                            }
                            // Otherwise, it's just a regular object that happened to parse as Literal
                            // Fall through to recursive processing
                        }
                    }
                }

                // Not an expression - process object recursively
                let map = value.as_object().unwrap(); // Safe because we're in Object match arm
                let mut result_map = serde_json::Map::new();
                for (k, v) in map {
                    match Box::pin(self.resolve_rec(v)).await? {
                        FlowResult::Success { result } => {
                            result_map.insert(k.clone(), result.as_ref().clone());
                        }
                        FlowResult::Skipped => {
                            return Ok(FlowResult::Skipped);
                        }
                        FlowResult::Failed { error } => {
                            return Ok(FlowResult::Failed { error });
                        }
                    }
                }
                Ok(FlowResult::Success {
                    result: ValueRef::new(serde_json::Value::Object(result_map)),
                })
            }
            serde_json::Value::Array(arr) => {
                // Process array recursively
                let mut result_array = Vec::new();
                for v in arr {
                    match Box::pin(self.resolve_rec(v)).await? {
                        FlowResult::Success { result } => {
                            result_array.push(result.as_ref().clone());
                        }
                        FlowResult::Skipped => {
                            return Ok(FlowResult::Skipped);
                        }
                        FlowResult::Failed { error } => {
                            return Ok(FlowResult::Failed { error });
                        }
                    }
                }
                Ok(FlowResult::Success {
                    result: ValueRef::new(serde_json::Value::Array(result_array)),
                })
            }
            _ => {
                // Return primitive values as-is
                Ok(FlowResult::Success {
                    result: ValueRef::new(value.clone()),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::workflow::Flow;
    use stepflow_state::{InMemoryStateStore, StateWriteOperation, StepResult};
    use uuid::Uuid;

    fn create_test_flow() -> Arc<Flow> {
        use stepflow_core::workflow::{Component, ErrorAction, Step};

        Arc::new(Flow {
            name: None,
            description: None,
            version: None,
            input_schema: None,
            output_schema: None,
            steps: vec![Step {
                id: "step1".to_string(),
                component: Component::from_string("mock://test"),
                input: ValueRef::new(json!({})),
                input_schema: None,
                output_schema: None,
                skip_if: None,
                on_error: ErrorAction::Fail,
            }],
            output: ValueRef::new(json!(null)),
            test: None,
            examples: vec![],
        })
    }

    #[tokio::test]
    async fn test_resolve_workflow_input() {
        let workflow_input = ValueRef::new(json!({"test": "hello", "number": 42}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        let write_cache = WriteCache::new(0);
        let flow = create_test_flow();
        let resolver = ValueResolver::new(
            execution_id,
            workflow_input.clone(),
            state_store,
            write_cache,
            flow,
        );

        // Test resolving workflow input
        let input_template = ValueRef::new(json!({"$from": {"workflow": "input"}}));
        let resolved = resolver.resolve(&input_template).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"test": "hello", "number": 42}));
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_workflow_input_with_path() {
        let workflow_input = ValueRef::new(json!({"name": "Alice", "age": 30}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        let write_cache = WriteCache::new(0);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving workflow input with path
        let input_template = ValueRef::new(json!({"$from": {"workflow": "input"}, "path": "name"}));
        let resolved = resolver.resolve(&input_template).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!("Alice"));
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_step_result() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store a step result
        let step_result = FlowResult::Success {
            result: ValueRef::new(json!({"output": "processed"})),
        };
        state_store
            .queue_write(StateWriteOperation::RecordStepResult {
                execution_id,
                step_result: StepResult::new(0, "step1", step_result),
            })
            .unwrap();

        // Ensure the write completes before proceeding
        state_store
            .flush_pending_writes(execution_id)
            .await
            .unwrap();

        let write_cache = WriteCache::new(1);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving step result
        let input_template = ValueRef::new(json!({"$from": {"step": "step1"}}));
        let resolved = resolver.resolve(&input_template).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"output": "processed"}));
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_step_result_with_path() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store a step result
        let step_result = FlowResult::Success {
            result: ValueRef::new(json!({"value": 123, "status": "ok"})),
        };
        state_store
            .queue_write(StateWriteOperation::RecordStepResult {
                execution_id,
                step_result: StepResult::new(0, "step1", step_result),
            })
            .unwrap();

        // Ensure the write completes before proceeding
        state_store
            .flush_pending_writes(execution_id)
            .await
            .unwrap();

        let write_cache = WriteCache::new(1);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving step result with path
        let input_template = ValueRef::new(json!({"$from": {"step": "step1"}, "path": "value"}));
        let resolved = resolver.resolve(&input_template).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!(123));
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_literal_wrapper() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        let write_cache = WriteCache::new(0);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving literal wrapper
        let input_template = ValueRef::new(json!({"$literal": {"special": "value"}}));
        let resolved = resolver.resolve(&input_template).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!({"special": "value"}));
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_complex_object() {
        let workflow_input = ValueRef::new(json!({"user": "Alice"}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store a step result
        let step_result = FlowResult::Success {
            result: ValueRef::new(json!({"count": 42})),
        };
        state_store
            .queue_write(StateWriteOperation::RecordStepResult {
                execution_id,
                step_result: StepResult::new(0, "step1", step_result),
            })
            .unwrap();

        // Ensure the write completes before proceeding
        state_store
            .flush_pending_writes(execution_id)
            .await
            .unwrap();

        let write_cache = WriteCache::new(1);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving complex object with multiple references
        let complex_input = ValueRef::new(json!({
            "user_name": {"$from": {"workflow": "input"}, "path": "user"},
            "result_count": {"$from": {"step": "step1"}, "path": "count"},
            "static_value": "hello",
            "nested": {
                "dynamic": {"$from": {"step": "step1"}},
                "static": "world"
            }
        }));

        let resolved = resolver.resolve(&complex_input).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                let expected = json!({
                    "user_name": "Alice",
                    "result_count": 42,
                    "static_value": "hello",
                    "nested": {
                        "dynamic": {"count": 42},
                        "static": "world"
                    }
                });
                assert_eq!(result.as_ref(), &expected);
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_array() {
        let workflow_input = ValueRef::new(json!({"items": ["a", "b"]}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        let write_cache = WriteCache::new(0);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving array with references
        let array_input = ValueRef::new(json!([
            {"$from": {"workflow": "input"}, "path": "items"},
            "static_item",
            {"$literal": ["literal", "array"]}
        ]));

        let resolved = resolver.resolve(&array_input).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                let expected = json!([["a", "b"], "static_item", ["literal", "array"]]);
                assert_eq!(result.as_ref(), &expected);
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_resolve_skipped_step_with_default() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store a skipped step result
        let step_result = FlowResult::Skipped;
        state_store
            .queue_write(StateWriteOperation::RecordStepResult {
                execution_id,
                step_result: StepResult::new(0, "step1", step_result),
            })
            .unwrap();

        // Ensure the write completes before proceeding
        state_store
            .flush_pending_writes(execution_id)
            .await
            .unwrap();

        let write_cache = WriteCache::new(1);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving with skip and default value using resolve_expr
        let expr = serde_json::from_value(json!({
            "$from": {"step": "step1"},
            "onSkip": {"action": "useDefault", "defaultValue": "fallback"}
        }))
        .unwrap();

        let resolved = resolver.resolve_expr(&expr).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                assert_eq!(result.as_ref(), &json!("fallback"));
            }
            _ => panic!(
                "Expected successful result with default value, got: {:?}",
                resolved
            ),
        }
    }

    #[tokio::test]
    async fn test_resolve_skipped_step_without_default() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store a skipped step result
        let step_result = FlowResult::Skipped;
        state_store
            .queue_write(StateWriteOperation::RecordStepResult {
                execution_id,
                step_result: StepResult::new(0, "step1", step_result),
            })
            .unwrap();

        // Ensure the write completes before proceeding
        state_store
            .flush_pending_writes(execution_id)
            .await
            .unwrap();

        let write_cache = WriteCache::new(1);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving with skip but no default - should propagate the skip
        let expr = serde_json::from_value(json!({"$from": {"step": "step1"}})).unwrap();
        let resolved = resolver.resolve_expr(&expr).await.unwrap();
        match resolved {
            FlowResult::Skipped => {
                // Expected - should propagate the skip when no default is provided
            }
            _ => panic!("Expected Skipped result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_workflow_output_skip_detection() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store a skipped step result
        let step_result = FlowResult::Skipped;
        state_store
            .queue_write(StateWriteOperation::RecordStepResult {
                execution_id,
                step_result: StepResult::new(0, "step1", step_result),
            })
            .unwrap();

        // Ensure the write completes before proceeding
        state_store
            .flush_pending_writes(execution_id)
            .await
            .unwrap();

        let write_cache = WriteCache::new(1);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test that workflow output is skipped when any dependency is skipped
        let output_template = ValueRef::new(json!({"result": {"$from": {"step": "step1"}}}));
        let output_result = resolver.resolve(&output_template).await.unwrap();

        match output_result {
            FlowResult::Skipped => {
                // Expected - workflow should be skipped
            }
            _ => panic!("Expected workflow to be skipped, got: {:?}", output_result),
        }
    }

    #[tokio::test]
    async fn test_resolve_primitive_values() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        let write_cache = WriteCache::new(0);
        let flow = create_test_flow();
        let resolver =
            ValueResolver::new(execution_id, workflow_input, state_store, write_cache, flow);

        // Test resolving primitive values - they should be returned as-is
        let number_template = ValueRef::new(json!(42));
        let number = resolver.resolve(&number_template).await.unwrap();
        match number {
            FlowResult::Success { result } => assert_eq!(result.as_ref(), &json!(42)),
            _ => panic!("Expected successful result"),
        }

        let string_template = ValueRef::new(json!("hello"));
        let string = resolver.resolve(&string_template).await.unwrap();
        match string {
            FlowResult::Success { result } => assert_eq!(result.as_ref(), &json!("hello")),
            _ => panic!("Expected successful result"),
        }

        let boolean_template = ValueRef::new(json!(true));
        let boolean = resolver.resolve(&boolean_template).await.unwrap();
        match boolean {
            FlowResult::Success { result } => assert_eq!(result.as_ref(), &json!(true)),
            _ => panic!("Expected successful result"),
        }

        let null_template = ValueRef::new(json!(null));
        let null = resolver.resolve(&null_template).await.unwrap();
        match null {
            FlowResult::Success { result } => assert_eq!(result.as_ref(), &json!(null)),
            _ => panic!("Expected successful result"),
        }
    }
}
