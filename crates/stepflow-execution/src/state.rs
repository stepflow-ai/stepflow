use std::collections::HashMap;

use crate::{ExecutionError, Result};
use error_stack::ResultExt;
use futures::{
    FutureExt as _, StreamExt, TryFutureExt as _,
    future::{BoxFuture, MaybeDone, Shared, maybe_done},
    stream::FuturesUnordered,
};
use indexmap::IndexMap;
use stepflow_core::workflow::{BaseRef, Expr, ValueRef};
use tokio::sync::oneshot;

#[derive(Clone)]
struct FutureValue(Shared<BoxFuture<'static, Result<ValueRef, oneshot::error::RecvError>>>);

impl FutureValue {
    fn literal(value: ValueRef) -> Self {
        Self(futures::future::ready(Ok(value)).boxed().shared())
    }

    fn computed() -> (Self, oneshot::Sender<ValueRef>) {
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

    fn get_value(
        &self,
        expr: &Expr,
    ) -> Result<impl Future<Output = Result<serde_json::Value>> + 'static> {
        debug_assert!(!matches!(expr, Expr::Literal { .. }));

        let base_ref = expr.base_ref().unwrap();
        let base = self
            .values
            .get(&base_ref)
            .ok_or(ExecutionError::UndefinedValue(base_ref.clone()))?
            .to_owned();

        // Ideally, we'd use an `Arc` for the field so it is cheaper to clone.
        let field = expr.field().map(|s| s.to_owned());
        Ok(async move {
            let base = base.0.await.change_context(ExecutionError::RecvInput)?;

            if let Some(field) = field {
                let value = base
                    .as_ref()
                    .get(&field)
                    .ok_or(ExecutionError::UndefinedField {
                        value: base.clone(),
                        field,
                    })?;
                tracing::info!("Returning {value:?}");
                Ok(value.clone())
            } else {
                tracing::info!("Returning {base:?}");
                Ok(base.as_ref().clone())
            }
        }
        .boxed())
    }

    /// Resolve any references in the given arguments.
    pub fn resolve(
        &self,
        args: &IndexMap<String, Expr>,
    ) -> Result<impl Future<Output = Result<ValueRef>> + 'static> {
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
                    let value = self.get_value(arg)?.map_ok(move |v| (name, v));
                    match maybe_done(value) {
                        MaybeDone::Future(value) => field_futures.push(value),
                        MaybeDone::Done(result) => {
                            let (name, value) = result?;
                            tracing::debug!(
                                "Resolved field {name:?} to already computed value {value:?}"
                            );
                            fields.insert(name, value);
                        }
                        MaybeDone::Gone => error_stack::bail!(ExecutionError::Internal),
                    }
                }
            }
        }

        Ok(async move {
            while let Some(next) = field_futures.next().await {
                let (name, value) = next?;
                tracing::debug!("Resolved field {name:?} to {value:?}");
                fields.insert(name, value);
            }
            let input = serde_json::Value::Object(fields);
            Ok(ValueRef::new(input))
        })
    }

    pub fn record_literal(&mut self, base_ref: BaseRef, value: ValueRef) -> Result<()> {
        self.values.insert(base_ref, FutureValue::literal(value));
        Ok(())
    }

    pub fn record_future(&mut self, base_ref: BaseRef) -> Result<oneshot::Sender<ValueRef>> {
        let (future, sender) = FutureValue::computed();
        self.values.insert(base_ref.clone(), future);
        Ok(sender)
    }
}
