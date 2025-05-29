use std::collections::{HashMap, HashSet};

use crate::{ExecutionError, Result};
use error_stack::ResultExt;
use futures::{
    FutureExt as _, TryStreamExt as _,
    future::{BoxFuture, Shared},
    stream::FuturesUnordered,
};
use stepflow_core::{
    FlowResult,
    workflow::{BaseRef, Expr, SkipAction, ValueRef},
};
use tokio::sync::oneshot;

#[derive(Clone)]
struct FutureValue(Shared<BoxFuture<'static, Result<FlowResult, oneshot::error::RecvError>>>);

impl FutureValue {
    fn literal(result: ValueRef) -> Self {
        Self(
            futures::future::ready(Ok(FlowResult::Success { result }))
                .boxed()
                .shared(),
        )
    }

    fn computed() -> (Self, oneshot::Sender<FlowResult>) {
        let (sender, receiver) = oneshot::channel();
        let f = receiver.boxed().shared();
        (Self(f), sender)
    }
}

pub struct State {
    /// Map from base references (input or step) to a future producing their FlowResult.
    flow_results: HashMap<BaseRef, FutureValue>,
}

impl State {
    pub fn new() -> Self {
        Self {
            flow_results: HashMap::new(),
        }
    }

    /// Get the value for an expression.
    pub fn resolve_expr(
        &self,
        expr: &Expr,
    ) -> Result<impl Future<Output = Result<FlowResult>> + 'static> {
        debug_assert!(!matches!(expr, Expr::Literal { .. }));

        let base_ref = expr.base_ref().expect("inputs handled earlier");
        let base = self
            .flow_results
            .get(base_ref)
            .ok_or(ExecutionError::UndefinedValue(base_ref.clone()))?
            .to_owned();

        // TODO: Can we avoid cloning the path?
        let path = expr.path().map(|p| p.to_owned());
        let on_skip = expr.on_skip().cloned();

        Ok(async move {
            let value = base.0.await.change_context(ExecutionError::RecvInput)?;

            match (value, path, on_skip) {
                (FlowResult::Success { result }, Some(path), _) => {
                    let result = result.path(&path).ok_or(ExecutionError::UndefinedField {
                        value: result.clone(),
                        field: path.to_owned(),
                    })?;
                    Ok(FlowResult::Success { result })
                }
                (FlowResult::Skipped, _, Some(SkipAction::UseDefault { default_value })) => {
                    // The step was skipped and we shold return a default value, so we return it.
                    // We expect the value to be nullable or for the default_value to be set.
                    let value = default_value.unwrap_or(serde_json::Value::Null.into());
                    Ok(FlowResult::Success { result: value })
                }
                (value, _, _) => Ok(value),
            }
        })
    }

    /// Resolve any references in a JSON value using a two-pass approach.
    /// First pass: collect unique base references that need to be resolved.
    /// Second pass: resolve the references and replace them in the JSON structure.
    pub fn resolve_object(
        &self,
        value: &serde_json::Value,
    ) -> Result<impl Future<Output = Result<FlowResult>> + 'static> {
        // First pass: collect unique base references and full expressions for skip handling
        let mut unique_base_refs = HashSet::new();
        collect_references_and_expressions(value, &mut unique_base_refs);

        tracing::info!(
            "Found {} unique base references to resolve",
            unique_base_refs.len()
        );

        // Prepare futures for all unique base references
        let mut reference_futures = FuturesUnordered::new();
        for base_ref in &unique_base_refs {
            let base_value = self
                .flow_results
                .get(base_ref)
                .ok_or(ExecutionError::UndefinedValue(base_ref.clone()))?
                .to_owned();

            let base_ref_clone = base_ref.clone();
            let future = async move {
                let result = base_value
                    .0
                    .await
                    .change_context(ExecutionError::RecvInput)?;
                Ok::<(BaseRef, FlowResult), error_stack::Report<ExecutionError>>((
                    base_ref_clone,
                    result,
                ))
            };
            reference_futures.push(future);
        }

        // Clone the value and expressions for the async block
        let value = value.clone();

        Ok(async move {
            // Collect all resolved base reference values
            let mut resolved_base_refs = HashMap::new();
            let mut any_skipped_or_failed = None;

            while let Some((base_ref, flow_result)) = reference_futures.try_next().await? {
                match flow_result {
                    FlowResult::Success { result } => {
                        resolved_base_refs.insert(base_ref, result);
                    }
                    other => {
                        // Store the first skip/failure for potential early return
                        if any_skipped_or_failed.is_none() {
                            any_skipped_or_failed = Some(other.clone());
                        }
                        // For skipped/failed references, we'll handle them during replacement
                        // based on their individual on_skip behavior
                    }
                }
            }

            // Second pass: replace references with resolved values, handling skip behavior
            let resolved_value = replace_references(&value, &resolved_base_refs)?;
            match resolved_value {
                Some(value) => Ok(FlowResult::Success {
                    result: value.into(),
                }),
                None => Ok(any_skipped_or_failed.unwrap_or(FlowResult::Skipped)),
            }
        })
    }

    pub fn record_literal(&mut self, base_ref: BaseRef, value: ValueRef) -> Result<()> {
        self.flow_results
            .insert(base_ref, FutureValue::literal(value));
        Ok(())
    }

    pub fn record_future(&mut self, base_ref: BaseRef) -> Result<oneshot::Sender<FlowResult>> {
        let (future, sender) = FutureValue::computed();
        self.flow_results.insert(base_ref.clone(), future);
        Ok(sender)
    }
}

/// Recursively collect all base references in a JSON value.
fn collect_references_and_expressions(value: &serde_json::Value, refs: &mut HashSet<BaseRef>) {
    match value {
        serde_json::Value::Object(map) => {
            // Check if this object is a literal wrapper
            if map.contains_key("$literal") {
                // Skip processing inside literal wrappers
                return;
            }

            // Check if this object is a reference
            if map.contains_key("$from") {
                // Try to deserialize this object as an Expr
                if let Ok(expr) = serde_json::from_value::<Expr>(value.clone()) {
                    if let Some(base_ref) = expr.base_ref() {
                        refs.insert(base_ref.clone());
                    }
                    return; // Don't recurse into reference objects
                }
            }

            // Not a reference or literal, recurse into all values
            for v in map.values() {
                collect_references_and_expressions(v, refs);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_references_and_expressions(v, refs);
            }
        }
        _ => {
            // Primitive values cannot contain references
        }
    }
}

/// Replace all references in a JSON value with their resolved values, handling skip behavior.
fn replace_references(
    value: &serde_json::Value,
    resolved_base_refs: &HashMap<BaseRef, ValueRef>,
) -> Result<Option<serde_json::Value>, error_stack::Report<crate::ExecutionError>> {
    match value {
        serde_json::Value::Object(map) => {
            // Check if this object is a literal wrapper
            if let Some(literal_content) = map.get("$literal") {
                // Unwrap the literal wrapper and return its contents without processing
                return Ok(Some(literal_content.clone()));
            }

            // Check if this object is a reference
            if map.contains_key("$from") {
                // Try to deserialize this object as an Expr to get the full reference
                if let Ok(expr) = serde_json::from_value::<Expr>(value.clone()) {
                    if let Some(base_ref) = expr.base_ref() {
                        if let Some(resolved_value) = resolved_base_refs.get(base_ref) {
                            // Successfully resolved, apply path if specified
                            let result = if let Some(path) = expr.path() {
                                if let Some(sub_value) = resolved_value.path(path) {
                                    sub_value.as_ref().clone()
                                } else {
                                    return Err(error_stack::report!(
                                        crate::ExecutionError::UndefinedField {
                                            value: resolved_value.clone(),
                                            field: path.to_string(),
                                        }
                                    ));
                                }
                            } else {
                                resolved_value.as_ref().clone()
                            };
                            return Ok(Some(result));
                        } else {
                            // This base reference was skipped or failed, handle on_skip behavior
                            if let Some(on_skip) = expr.on_skip() {
                                match on_skip {
                                    SkipAction::Skip => {
                                        // Propagate the skip
                                        return Ok(None);
                                    }
                                    SkipAction::UseDefault { default_value } => {
                                        // Use the default value
                                        let default = default_value
                                            .to_owned()
                                            .unwrap_or(serde_json::Value::Null.into());
                                        return Ok(Some(default.as_ref().clone()));
                                    }
                                }
                            } else {
                                // No skip action specified, propagate the skip/failure
                                return Ok(None);
                            }
                        }
                    }
                }
            }

            // Not a reference or literal, recurse into all values
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                match replace_references(v, resolved_base_refs)? {
                    Some(resolved) => new_map.insert(k.clone(), resolved),
                    None => return Ok(None), // Propagate skip
                };
            }
            Ok(Some(serde_json::Value::Object(new_map)))
        }
        serde_json::Value::Array(arr) => {
            let mut new_arr = Vec::new();
            for v in arr {
                match replace_references(v, resolved_base_refs)? {
                    Some(resolved) => new_arr.push(resolved),
                    None => return Ok(None), // Propagate skip
                }
            }
            Ok(Some(serde_json::Value::Array(new_arr)))
        }
        _ => {
            // Primitive values are returned as-is
            Ok(Some(value.clone()))
        }
    }
}
