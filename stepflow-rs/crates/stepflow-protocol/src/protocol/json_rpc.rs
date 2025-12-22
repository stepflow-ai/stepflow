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

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

/// The JSON-RPC protocol version.
#[derive(Debug, Default)]
pub struct JsonRpc;

impl PartialSchema for JsonRpc {
    fn schema() -> RefOr<Schema> {
        RefOr::T(Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Type(Type::String))
                .description(Some("The version of the JSON-RPC protocol."))
                .default(Some(serde_json::json!("2.0")))
                .enum_values(Some(["2.0"]))
                .build(),
        ))
    }
}

impl ToSchema for JsonRpc {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("JsonRpc")
    }
}

impl Serialize for JsonRpc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let version: &str = Deserialize::deserialize(deserializer)?;
        if version == "2.0" {
            Ok(JsonRpc)
        } else {
            Err(serde::de::Error::custom(format!(
                "Invalid JSON-RPC version {version:?}"
            )))
        }
    }
}
