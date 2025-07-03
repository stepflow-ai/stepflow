// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::borrow::Cow;

use error_stack::ResultExt as _;
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Deserializer, Serialize};

pub(crate) trait ErasedSerializeDebug: erased_serde::Serialize + std::fmt::Debug {}
impl<T: erased_serde::Serialize + std::fmt::Debug> ErasedSerializeDebug for T {}

#[derive(Debug)]
pub enum LazyValue<'a> {
    /// A value that can be efficiently serialized.
    Write(&'a (dyn ErasedSerializeDebug + Send + Sync)),
    /// A raw value that has not yet been deserialized.
    Read(&'a serde_json::value::RawValue),
}

impl<'a> JsonSchema for LazyValue<'a> {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("Value")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> Schema {
        schemars::Schema::from(true)
    }

    fn inline_schema() -> bool {
        true
    }
}

impl<'a> Serialize for LazyValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            LazyValue::Write(serializable) => {
                let serializable: &dyn erased_serde::Serialize = *serializable;
                erased_serde::serialize(serializable, serializer)
            }
            LazyValue::Read(raw_value) => raw_value.serialize(serializer),
        }
    }
}

impl<'a, 'de: 'a> Deserialize<'de> for LazyValue<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize directly to RawValue for lazy evaluation
        let raw = <&'a serde_json::value::RawValue>::deserialize(deserializer)?;
        Ok(LazyValue::Read(raw))
    }
}

impl<'a> LazyValue<'a> {
    /// Create a LazyValue from a serializable value
    pub fn write_ref<T: serde::Serialize + std::fmt::Debug + Sync + Send>(value: &'a T) -> Self {
        LazyValue::Write(value)
    }

    /// Deserialize the lazy value to a concrete type.
    /// Only works for Read variant, returns error for Write variant.
    pub fn deserialize_to<T>(&self) -> error_stack::Result<T, LazyValueError>
    where
        T: serde::de::DeserializeOwned,
    {
        match self {
            LazyValue::Write(_) => {
                error_stack::bail!(LazyValueError::CannotDeserializeFromWrite)
            }
            LazyValue::Read(raw) => serde_json::from_str(raw.get())
                .change_context_lazy(|| LazyValueError::DeserializationError(raw.get().to_owned())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LazyValueError {
    #[error("Cannot deserialize from Write variant - use the original value instead")]
    CannotDeserializeFromWrite,
    #[error("Unable to deserialize from '{0}'")]
    DeserializationError(String),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_lazy_value_deserialize_to() {
        // Test successful deserialization from Read variant created by JSON deserialization
        let json_str = r#"{"key": "value", "number": 42}"#;
        let lazy_value: LazyValue<'_> = serde_json::from_str(json_str).unwrap();

        let result: serde_json::Value = lazy_value.deserialize_to().unwrap();
        assert_eq!(result["key"], "value");
        assert_eq!(result["number"], 42);

        // Test error from Write variant
        let test_data = json!({"test": "data"});
        let lazy_value = LazyValue::write_ref(&test_data);
        let result: Result<serde_json::Value, _> = lazy_value.deserialize_to();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err().current_context(),
            LazyValueError::CannotDeserializeFromWrite
        ));
    }

    #[test]
    fn test_lazy_value_serialization() {
        // Test serialization of Write variant
        let test_data = json!({"test": "write_variant"});
        let lazy_write = LazyValue::write_ref(&test_data);
        let serialized = serde_json::to_string(&lazy_write).unwrap();
        assert_eq!(serialized, r#"{"test":"write_variant"}"#);

        // Test serialization of Read variant created by deserialization
        let json_str = r#"{"test":"read_variant"}"#;
        let lazy_read: LazyValue<'_> = serde_json::from_str(json_str).unwrap();
        let serialized = serde_json::to_string(&lazy_read).unwrap();
        assert_eq!(serialized, r#"{"test":"read_variant"}"#);
    }

    #[test]
    fn test_lazy_value_deserialization() {
        let json_str = r#"{"key": "value", "number": 42}"#;
        let lazy_value: LazyValue<'_> = serde_json::from_str(json_str).unwrap();

        // Should always deserialize to Read variant
        match lazy_value {
            LazyValue::Read(raw) => {
                assert_eq!(raw.get(), json_str);
            }
            LazyValue::Write(_) => panic!("Expected Read variant"),
        }

        // Test that we can deserialize to concrete type
        let result: serde_json::Value = lazy_value.deserialize_to().unwrap();
        assert_eq!(result["key"], "value");
        assert_eq!(result["number"], 42);
    }

    #[derive(Deserialize)]
    struct TestStruct<'a> {
        #[serde(default, borrow)]
        value: Option<LazyValue<'a>>,
    }

    #[test]
    fn test_lazy_value_nested_deserialization() {
        let json_str = r#"{"value": {"key": "nested_value", "number": 100}}"#;
        let test_struct: TestStruct<'_> = serde_json::from_str(json_str).unwrap();

        // Should be able to deserialize nested LazyValue
        let result: serde_json::Value = test_struct.value.unwrap().deserialize_to().unwrap();
        assert_eq!(result["key"], "nested_value");
        assert_eq!(result["number"], 100);

        // Should be able to deserialize when absent as well
        let json_str = r#"{}"#;
        let test_struct: TestStruct<'_> = serde_json::from_str(json_str).unwrap();
        assert!(test_struct.value.is_none());
    }
}
