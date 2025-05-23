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
    workflow::{BaseRef, Expr, ValueRef},
};
use tokio::sync::oneshot;

#[derive(Clone)]
struct FutureValue(Shared<BoxFuture<'static, Result<FlowResult, oneshot::error::RecvError>>>);

impl FutureValue {
    fn literal(value: ValueRef) -> Self {
        Self(
            futures::future::ready(Ok(FlowResult::Success(value)))
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
    values: HashMap<BaseRef, FutureValue>,
}

impl State {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Get the value for an expression.
    fn get_value(&self, expr: &Expr) -> Result<impl Future<Output = Result<FlowResult>> + 'static> {
        debug_assert!(!matches!(expr, Expr::Literal { .. }));

        let base_ref = expr.base_ref().expect("inputs handled earlier");
        let base = self
            .values
            .get(base_ref)
            .ok_or(ExecutionError::UndefinedValue(base_ref.clone()))?
            .to_owned();

        // TODO: Can we avoid cloning the path?
        let path = expr.path().map(|p| p.to_owned());

        Ok(async move {
            let value = base.0.await.change_context(ExecutionError::RecvInput)?;

            match (value, path) {
                (FlowResult::Success(value), Some(path)) => {
                    let value = value.path(&path).ok_or(ExecutionError::UndefinedField {
                        value: value.clone(),
                        field: path.to_owned(),
                    })?;
                    Ok(FlowResult::Success(value))
                }
                (value, _) => Ok(value),
            }
        })
    }

    /// Resolve any references in the given arguments.
    pub fn resolve(
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
                    field_futures.push(self.get_value(arg)?.map_ok(move |v| (name, v)));
                }
            }
        }

        Ok(async move {
            while let Some((name, value)) = field_futures.try_next().await? {
                tracing::debug!("Resolved field {name:?} to {value:?}");
                match value {
                    FlowResult::Success(value) => {
                        fields.insert(name, value.as_ref().clone());
                    }
                    other => return Ok(other),
                }
            }

            let input = serde_json::Value::Object(fields);
            Ok(FlowResult::Success(ValueRef::new(input)))
        })
    }

    pub fn record_literal(&mut self, base_ref: BaseRef, value: ValueRef) -> Result<()> {
        self.values.insert(base_ref, FutureValue::literal(value));
        Ok(())
    }

    pub fn record_future(&mut self, base_ref: BaseRef) -> Result<oneshot::Sender<FlowResult>> {
        let (future, sender) = FutureValue::computed();
        self.values.insert(base_ref.clone(), future);
        Ok(sender)
    }
}
