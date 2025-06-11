use std::{collections::HashMap, sync::Arc};

use stepflow_core::{
    FlowResult,
    workflow::{BaseRef, Expr, SkipAction, ValueRef, ExpressionReference, ReferenceLocation, extract_references_from_value_ref, extract_reference_from_expr, WorkflowRef},
};
use stepflow_state::StateStore;
use uuid::Uuid;

use crate::{ExecutionError, Result};

/// Enhanced value resolver that uses the shared analysis module
/// 
/// This demonstrates how edge extraction and value resolution can share
/// common functionality for analyzing workflow expressions.
pub struct EnhancedValueResolver {
    /// Execution ID of the workflow we are resolving for.
    execution_id: Uuid,
    /// Input value for the workflow.
    input: ValueRef,
    /// State store to use for resolving values.
    state_store: Arc<dyn StateStore>,
    /// Cache of resolved references to avoid repeated state store lookups
    reference_cache: Arc<tokio::sync::RwLock<HashMap<String, FlowResult>>>,
}

impl EnhancedValueResolver {
    pub fn new(execution_id: Uuid, input: ValueRef, state_store: Arc<dyn StateStore>) -> Self {
        Self {
            execution_id,
            input,
            state_store,
            reference_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Resolve a ValueRef, returning a FlowResult.
    /// This version uses the shared analysis module to extract references first.
    pub async fn resolve(&self, value: &ValueRef) -> Result<FlowResult> {
        // Extract all references from the value using the shared analysis module
        let location = ReferenceLocation::new("value_resolution");
        let references = extract_references_from_value_ref(value, location);
        
        // Batch resolve all step references to optimize state store interactions
        self.batch_resolve_references(&references).await?;
        
        // Now resolve the value recursively
        self.resolve_rec(value.as_ref()).await
    }

    /// Batch resolve multiple references to optimize state store access
    async fn batch_resolve_references(&self, references: &[ExpressionReference]) -> Result<()> {
        // Extract unique step dependencies
        let step_ids: Vec<String> = references
            .iter()
            .filter_map(|reference| {
                if let BaseRef::Step { step } = &reference.base_ref {
                    Some(step.clone())
                } else {
                    None
                }
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Check which references are already cached
        let cache = self.reference_cache.read().await;
        let uncached_step_ids: Vec<String> = step_ids
            .into_iter()
            .filter(|step_id| !cache.contains_key(step_id))
            .collect();
        drop(cache);

        if uncached_step_ids.is_empty() {
            return Ok(());
        }

        // Batch fetch uncached step results
        // For now, we'll use individual calls, but this could be optimized with 
        // the new compound StateStore methods like get_step_results_by_indices
        let mut cache = self.reference_cache.write().await;
        for step_id in uncached_step_ids {
            match self.state_store.get_step_result_by_id(self.execution_id, &step_id).await {
                Ok(result) => {
                    cache.insert(step_id, result);
                }
                Err(_) => {
                    // We'll handle the error during actual resolution
                    // For now, just note that this step is not available
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Get a cached step result, falling back to state store if not cached
    async fn get_step_result(&self, step_id: &str) -> Result<FlowResult> {
        // Try cache first
        {
            let cache = self.reference_cache.read().await;
            if let Some(result) = cache.get(step_id) {
                return Ok(result.clone());
            }
        }

        // Fall back to state store
        match self.state_store.get_step_result_by_id(self.execution_id, step_id).await {
            Ok(result) => {
                // Cache the result for future use
                let mut cache = self.reference_cache.write().await;
                cache.insert(step_id.to_string(), result.clone());
                Ok(result)
            }
            Err(_) => Err(ExecutionError::UndefinedValue(BaseRef::Step { step: step_id.to_string() }).into()),
        }
    }

    /// Resolve an expression using the shared analysis module
    pub async fn resolve_expr(&self, expr: &Expr) -> Result<FlowResult> {
        // Handle literal expressions
        if let Expr::Literal(value) = expr {
            return Ok(FlowResult::Success {
                result: value.clone(),
            });
        }

        // Use the shared analysis module to extract the reference
        let location = ReferenceLocation::new("expression_resolution");
        if let Some(reference) = extract_reference_from_expr(expr, location) {
            let base_result = match &reference.base_ref {
                BaseRef::Workflow(WorkflowRef::Input) => {
                    // Return the workflow input
                    FlowResult::Success {
                        result: self.input.clone(),
                    }
                }
                BaseRef::Step { step } => {
                    // Get step result using cached resolution
                    self.get_step_result(step).await?
                }
            };

            // Apply path if specified
            let path_result = if let Some(path) = &reference.path {
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

            // Handle skip actions
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
        } else {
            Err(ExecutionError::Internal.into())
        }
    }

    /// Recursive resolution of JSON values, optimized with shared analysis
    async fn resolve_rec(&self, value: &serde_json::Value) -> Result<FlowResult> {
        match value {
            serde_json::Value::Object(fields) => {
                // Try to parse as an expression first
                if let Ok(expr) = serde_json::from_value::<Expr>(value.clone()) {
                    match expr {
                        Expr::Ref { .. } => {
                            // It's a reference - resolve it using the enhanced method
                            return self.resolve_expr(&expr).await;
                        }
                        Expr::Literal(_) => {
                            // Check if this is actually a $literal wrapper
                            if let Some(literal_content) = fields.get("$literal") {
                                // It's a literal wrapper - return the unwrapped value
                                return Ok(FlowResult::Success {
                                    result: ValueRef::new(literal_content.clone()),
                                });
                            }
                            // Otherwise, fall through to recursive processing
                        }
                    }
                }

                // Not an expression - process object recursively
                let mut result_map = serde_json::Map::new();
                for (k, v) in fields {
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

    /// Analyze a value for its dependencies (demonstration of shared analysis)
    pub fn analyze_dependencies(&self, value: &ValueRef) -> Vec<String> {
        let location = ReferenceLocation::new("dependency_analysis");
        let references = extract_references_from_value_ref(value, location);
        
        stepflow_core::workflow::extract_step_dependencies(&references)
            .into_iter()
            .collect()
    }

    /// Pre-warm the cache by analyzing the entire value and fetching dependencies
    pub async fn pre_warm_cache(&self, value: &ValueRef) -> Result<()> {
        let location = ReferenceLocation::new("cache_warming");
        let references = extract_references_from_value_ref(value, location);
        self.batch_resolve_references(&references).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_state::{InMemoryStateStore, StepResult};

    #[tokio::test]
    async fn test_enhanced_resolve_with_cache() {
        let workflow_input = ValueRef::new(json!({"user": "Alice"}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        // Store some step results
        let step1_result = FlowResult::Success {
            result: ValueRef::new(json!({"count": 42})),
        };
        let step2_result = FlowResult::Success {
            result: ValueRef::new(json!({"processed": true})),
        };
        
        state_store
            .record_step_result(execution_id, StepResult::new(0, "step1", step1_result))
            .await
            .unwrap();
        state_store
            .record_step_result(execution_id, StepResult::new(1, "step2", step2_result))
            .await
            .unwrap();

        let resolver = EnhancedValueResolver::new(execution_id, workflow_input, state_store);

        // Test resolving complex object that references both steps
        let complex_input = ValueRef::new(json!({
            "user_name": {"$from": {"workflow": "input"}, "path": "user"},
            "result1": {"$from": {"step": "step1"}, "path": "count"},
            "result2": {"$from": {"step": "step2"}, "path": "processed"},
            "combined": {
                "step1_data": {"$from": {"step": "step1"}},
                "step2_data": {"$from": {"step": "step2"}}
            }
        }));

        // Pre-warm the cache (this demonstrates the shared analysis)
        resolver.pre_warm_cache(&complex_input).await.unwrap();

        // Resolve the complex value
        let resolved = resolver.resolve(&complex_input).await.unwrap();
        match resolved {
            FlowResult::Success { result } => {
                let expected = json!({
                    "user_name": "Alice",
                    "result1": 42,
                    "result2": true,
                    "combined": {
                        "step1_data": {"count": 42},
                        "step2_data": {"processed": true}
                    }
                });
                assert_eq!(result.as_ref(), &expected);
            }
            _ => panic!("Expected successful result, got: {:?}", resolved),
        }
    }

    #[tokio::test]
    async fn test_dependency_analysis() {
        let workflow_input = ValueRef::new(json!({}));
        let state_store: Arc<dyn StateStore> = Arc::new(InMemoryStateStore::new());
        let execution_id = Uuid::new_v4();

        let resolver = EnhancedValueResolver::new(execution_id, workflow_input, state_store);

        // Test dependency analysis
        let complex_value = ValueRef::new(json!({
            "input1": {"$from": {"step": "step1"}, "path": "result"},
            "input2": {"$from": {"workflow": "input"}, "path": "user_data"},
            "nested": {
                "step2_ref": {"$from": {"step": "step2"}},
                "step3_ref": {"$from": {"step": "step3"}, "path": "output"}
            }
        }));

        let dependencies = resolver.analyze_dependencies(&complex_value);
        
        assert_eq!(dependencies.len(), 3);
        assert!(dependencies.contains(&"step1".to_string()));
        assert!(dependencies.contains(&"step2".to_string()));
        assert!(dependencies.contains(&"step3".to_string()));
    }
}