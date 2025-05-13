use std::collections::HashMap;

use crate::{ExecutionError, Result};
use indexmap::IndexMap;
use stepflow_workflow::{BaseRef, Expr, Value};

pub struct VecState {
    pub values: HashMap<BaseRef, Value>,
}

impl VecState {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    fn get_value<'a>(&mut self, expr: &'a Expr) -> Result<serde_json::Value> {
        if let Expr::Literal { literal } = expr {
            return Ok(literal.as_ref().clone());
        }

        let base_ref = expr.base_ref().unwrap();
        let base = self
            .values
            .get(&base_ref)
            .ok_or(ExecutionError::UndefinedValue(base_ref.clone()))?;

        if let Some(field) = expr.field() {
            let value = base
                .as_ref()
                .get(field)
                .ok_or(ExecutionError::UndefinedField {
                    value: base.clone(),
                    field: field.to_owned(),
                })?;
            Ok(value.clone())
        } else {
            Ok(base.as_ref().clone())
        }
    }

    /// Resolve any references in the given arguments.
    pub fn resolve(&mut self, args: &IndexMap<String, Expr>) -> Result<Value> {
        let inputs = args
            .iter()
            .map(|(name, arg)| {
                let value = self.get_value(arg)?;
                Ok((name.to_owned(), value))
            })
            .collect::<Result<serde_json::Map<String, serde_json::Value>>>()?;
        let input = serde_json::Value::Object(inputs);
        Ok(Value::new(input))
    }

    /// Record results for the given step.
    pub fn record_value(&mut self, base_ref: BaseRef, result: Value) -> Result<()> {
        self.values.insert(base_ref, result);
        Ok(())
    }
}
