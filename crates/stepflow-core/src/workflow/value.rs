use std::sync::Arc;

use owning_ref::ArcRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A literal value which may be passed to a component.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
#[repr(transparent)]
pub struct ValueRef(ArcRef<'static, serde_json::Value>);

impl JsonSchema for ValueRef {
    fn schema_name() -> String {
        "ValueRef".to_string()
    }

    fn json_schema(generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        serde_json::Value::json_schema(generator)
    }
}

impl Serialize for ValueRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_ref().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ValueRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_json::Value::deserialize(deserializer).map(Self::new)
    }
}

impl<T: Into<serde_json::Value>> From<T> for ValueRef {
    fn from(value: T) -> Self {
        Self::new(value.into())
    }
}

impl ValueRef {
    pub fn new(value: serde_json::Value) -> Self {
        Self(ArcRef::new(Arc::new(value)))
    }

    fn maybe_map(
        &self,
        f: impl FnOnce(&serde_json::Value) -> Option<&serde_json::Value>,
    ) -> Option<ValueRef> {
        let result = self.0.clone().try_map(move |o| f(o).ok_or(()));
        match result {
            Ok(value) => Some(Self(value)),
            Err(()) => None,
        }
    }

    /// Access the given field as a `ValueRef`.
    ///
    /// NOTE: This increments the references to the root value. If the resulting
    /// field reference is significantly smaller than the root value and likely
    /// to outlive it, consider instead creating a new (rooted) value.
    pub fn field(&self, field: &str) -> Option<ValueRef> {
        self.maybe_map(|o| o.as_object().and_then(|o| o.get(field)))
    }
}

impl AsRef<serde_json::Value> for ValueRef {
    fn as_ref(&self) -> &serde_json::Value {
        &self.0
    }
}
