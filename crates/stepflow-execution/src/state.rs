use std::collections::HashMap;

use crate::{ExecutionError, Result};
use error_stack::ResultExt;
use futures::{
    FutureExt as _, TryFutureExt as _, TryStreamExt as _,
    future::{BoxFuture, Shared},
    stream::FuturesUnordered,
};
use indexmap::IndexMap;
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

    /// Resolve any references in the given arguments.
    pub fn resolve_object(
        &self,
        args: &IndexMap<String, Expr>,
    ) -> Result<impl Future<Output = Result<FlowResult>> + 'static> {
        tracing::info!("Resolving {args:?}");
        let mut fields = serde_json::Map::with_capacity(args.len());
        let mut field_futures = FuturesUnordered::new();
        for (name, arg) in args {
            let name = name.to_owned();

            match arg {
                Expr::Literal(literal) => {
                    tracing::debug!("Resolved field {name:?} to literal {literal:?}");
                    fields.insert(name, literal.as_ref().clone());
                }
                _ => {
                    field_futures.push(self.resolve_expr(arg)?.map_ok(move |v| (name, v)));
                }
            }
        }

        Ok(async move {
            while let Some((name, value)) = field_futures.try_next().await? {
                tracing::debug!("Resolved field {name:?} to {value:?}");
                match value {
                    FlowResult::Success { result } => {
                        fields.insert(name, result.as_ref().clone());
                    }
                    other => return Ok(other),
                }
            }

            let input = serde_json::Value::Object(fields);
            Ok(input.into())
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
