//! Schema manipulation and validation types.

use schemars::{
    JsonSchema,
    schema::{Schema, SchemaObject},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Type alias for a shared schema reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(transparent)]
pub struct SchemaRef(Arc<SchemaObject>);

impl From<Schema> for SchemaRef {
    fn from(schema: Schema) -> Self {
        SchemaRef(Arc::new(schema.into_object()))
    }
}

impl AsRef<SchemaObject> for SchemaRef {
    fn as_ref(&self) -> &SchemaObject {
        &self.0
    }
}

impl SchemaRef {
    /// Create a schema reference from a type that implements JsonSchema.
    pub fn for_type<T: JsonSchema>() -> Self {
        // TODO: Look into caching this? We could use the `schema_id`?
        let mut generator = schemars::SchemaGenerator::default();
        let schema = T::json_schema(&mut generator);
        schema.into()
    }

    pub fn parse_json(s: &str) -> Result<Self, serde_json::Error> {
        let schema = serde_json::from_str::<Schema>(s)?;
        Ok(schema.into())
    }
}
