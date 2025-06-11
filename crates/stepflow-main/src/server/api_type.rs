use std::borrow::Cow;

use poem_openapi::registry::{MetaSchema, MetaSchemaRef};
use poem_openapi::types::{ParseError, ParseFromJSON, ParseResult, ToJSON, Type};
use schemars::{JsonSchema, schema::Schema};
use serde::{Serialize, de::DeserializeOwned};

/// A generic wrapper that automatically implements poem-openapi traits
/// for any type that implements Serialize + DeserializeOwned + JsonSchema.
///
/// # Example
///
/// ```rust
/// use stepflow_main::server::api_type::ApiType;
/// use stepflow_core::FlowResult;
///
/// // Instead of manually implementing poem-openapi traits for FlowResult:
/// pub type ApiFlowResult = ApiType<FlowResult>;
///
/// // Now ApiFlowResult can be used directly in poem-openapi endpoints
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ApiType<T>(pub T);

impl<T> ApiType<T> {
    /// Create a new wrapper around the value
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Extract the wrapped value
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get a reference to the wrapped value
    pub fn get_ref(&self) -> &T {
        &self.0
    }

    /// Get a mutable reference to the wrapped value
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> std::ops::Deref for ApiType<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for ApiType<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for ApiType<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

// Serde implementations - delegate to the wrapped type
impl<T: Serialize> Serialize for ApiType<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, T: DeserializeOwned> serde::Deserialize<'de> for ApiType<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Self::new)
    }
}

// JsonSchema implementation - delegate to the wrapped type
impl<T: JsonSchema> JsonSchema for ApiType<T> {
    fn schema_name() -> String {
        T::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }

    fn is_referenceable() -> bool {
        T::is_referenceable()
    }
}

// poem-openapi trait implementations
impl<T> Type for ApiType<T>
where
    T: JsonSchema + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    const IS_REQUIRED: bool = true;

    type RawValueType = Self;
    type RawElementValueType = Self;

    fn name() -> Cow<'static, str> {
        T::schema_name().into()
    }

    fn schema_ref() -> MetaSchemaRef {
        // Try to get the schema from JsonSchema implementation
        let mut generator = schemars::SchemaGenerator::default();
        let schema = T::json_schema(&mut generator);

        // Convert schemars::Schema to poem_openapi::registry::MetaSchema
        let meta_schema = convert_schemars_to_meta_schema(schema, &T::schema_name());

        if T::is_referenceable() {
            MetaSchemaRef::Reference(T::schema_name())
        } else {
            MetaSchemaRef::Inline(Box::new(meta_schema))
        }
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }

    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        Box::new(std::iter::once(self))
    }
}

impl<T> ParseFromJSON for ApiType<T>
where
    T: DeserializeOwned + Serialize + JsonSchema + Send + Sync + 'static,
{
    fn parse_from_json(value: Option<serde_json::Value>) -> ParseResult<Self> {
        match value {
            Some(v) => serde_json::from_value::<T>(v)
                .map(Self::new)
                .map_err(|e| ParseError::custom(e.to_string())),
            None => Err(ParseError::expected_input()),
        }
    }
}

impl<T> ToJSON for ApiType<T>
where
    T: Serialize + DeserializeOwned + JsonSchema + Send + Sync + 'static,
{
    fn to_json(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.0).ok()
    }
}

/// Convert a schemars::Schema to poem_openapi::registry::MetaSchema
/// This is a best-effort conversion for basic schema properties
fn convert_schemars_to_meta_schema(
    _schema: schemars::schema::Schema,
    type_name: &str,
) -> MetaSchema {
    // For simplicity, we'll create a basic schema with just the type name
    // More sophisticated conversion could be added later if needed
    MetaSchema {
        title: Some(type_name.to_string()),
        description: None, // We can't easily create a &'static str from format
        example: Some(serde_json::json!({
            "type": type_name.to_lowercase()
        })),
        ..MetaSchema::ANY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    struct TestStruct {
        pub name: String,
        pub count: i32,
    }

    #[test]
    fn test_api_type_basic_functionality() {
        let original = TestStruct {
            name: "test".to_string(),
            count: 42,
        };

        let wrapped = ApiType::new(original.clone());

        // Test deref
        assert_eq!(wrapped.name, "test");
        assert_eq!(wrapped.count, 42);

        // Test into_inner
        let extracted = wrapped.into_inner();
        assert_eq!(extracted, original);
    }

    #[test]
    fn test_api_type_serde() {
        let original = TestStruct {
            name: "test".to_string(),
            count: 42,
        };

        let wrapped = ApiType::new(original.clone());

        // Test serialization
        let json = serde_json::to_string(&wrapped).unwrap();
        let original_json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, original_json);

        // Test deserialization
        let deserialized: ApiType<TestStruct> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.into_inner(), original);
    }

    #[test]
    fn test_api_type_schema() {
        // Test that JsonSchema delegation works
        let wrapper_schema_name = ApiType::<TestStruct>::schema_name();
        let original_schema_name = TestStruct::schema_name();
        assert_eq!(wrapper_schema_name, original_schema_name);
    }

    #[test]
    fn test_api_type_poem_traits() {
        // Test Type trait
        let type_name = ApiType::<TestStruct>::name();
        assert_eq!(type_name, TestStruct::schema_name());

        // Test ToJSON
        let original = TestStruct {
            name: "test".to_string(),
            count: 42,
        };
        let wrapped = ApiType::new(original);

        let json_value = wrapped.to_json().unwrap();
        assert_eq!(json_value["name"], "test");
        assert_eq!(json_value["count"], 42);

        // Test ParseFromJSON
        let parsed = ApiType::<TestStruct>::parse_from_json(Some(json_value)).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.count, 42);
    }
}
