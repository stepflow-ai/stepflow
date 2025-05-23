use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A literal value which may be passed to a component.

// TODO: Change value representation to avoid copying?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Hash, Eq)]
#[repr(transparent)]
pub struct Value(Arc<serde_json::Value>);

impl<T: Into<serde_json::Value>> From<T> for Value {
    fn from(value: T) -> Self {
        Self::new(value.into())
    }
}

impl Value {
    pub fn new(value: serde_json::Value) -> Self {
        Self(Arc::new(value))
    }
}

impl AsRef<serde_json::Value> for Value {
    fn as_ref(&self) -> &serde_json::Value {
        &self.0
    }
}
