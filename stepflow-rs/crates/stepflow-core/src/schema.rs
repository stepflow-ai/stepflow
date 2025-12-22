// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

//! Schema manipulation and validation types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::Arc;
use utoipa::{PartialSchema, ToSchema};

use crate::json_schema::generate_json_schema;

/// A shared reference to a JSON Schema.
///
/// This wraps a `serde_json::Value` that represents a JSON Schema.
/// The value should be either an object or a boolean (the valid top-level
/// JSON Schema values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[repr(transparent)]
pub struct SchemaRef(Arc<Value>);

impl From<Value> for SchemaRef {
    fn from(value: Value) -> Self {
        SchemaRef(Arc::new(value))
    }
}

impl AsRef<Value> for SchemaRef {
    fn as_ref(&self) -> &Value {
        &self.0
    }
}

impl utoipa::PartialSchema for SchemaRef {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        // OpenAPI doesn't allow external references, so there is no good way to
        // enforce that this is consistent with the JSON schema.
        let schema = utoipa::openapi::ObjectBuilder::new()
            .description(Some("A valid JSON Schema object."))
            .build();
        utoipa::openapi::RefOr::T(utoipa::openapi::Schema::Object(schema))
    }
}

impl utoipa::ToSchema for SchemaRef {
    fn name() -> Cow<'static, str> {
        // Use "Schema" as the semantic name (the "Ref" is an implementation detail)
        Cow::Borrowed("Schema")
    }
}

impl SchemaRef {
    /// Create a schema reference from a type that implements ToSchema.
    pub fn for_type<T: ToSchema + PartialSchema>() -> Self {
        let json_schema = generate_json_schema::<T>();
        json_schema.into()
    }

    /// Parse a JSON Schema from a JSON string.
    pub fn parse_json(s: &str) -> Result<Self, serde_json::Error> {
        let value = serde_json::from_str::<Value>(s)?;
        Ok(value.into())
    }

    /// Get the schema as a JSON value reference.
    pub fn as_value(&self) -> &Value {
        &self.0
    }
}
