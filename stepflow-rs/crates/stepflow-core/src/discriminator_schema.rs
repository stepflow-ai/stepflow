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

//! Reusable schemars transforms for adding OpenAPI discriminators to tagged enum schemas.

use serde_json::{Map, Value};

/// A schemars [`Transform`](schemars::transform::Transform) that adds an OpenAPI
/// `discriminator` object to `oneOf` schemas generated from `#[serde(tag = "...")]` enums.
///
/// # Usage
///
/// ```ignore
/// #[derive(schemars::JsonSchema, serde::Serialize, serde::Deserialize)]
/// #[serde(tag = "type", rename_all = "camelCase")]
/// #[schemars(transform = AddDiscriminator::new("type"))]
/// enum MyEnum {
///     #[schemars(title = "VariantA")]
///     VariantA,
///     #[schemars(title = "VariantB")]
///     VariantB { value: String },
/// }
/// ```
pub struct AddDiscriminator {
    property_name: String,
}

impl AddDiscriminator {
    /// Create a new `AddDiscriminator` transform for the given tag property name.
    ///
    /// The `property_name` should match the `tag` value in `#[serde(tag = "...")]`.
    pub fn new(property_name: impl Into<String>) -> Self {
        Self {
            property_name: property_name.into(),
        }
    }

    /// Extract the discriminator tag's `const` value from a oneOf variant schema.
    fn find_tag_const<'a>(variant: &'a Value, tag_property: &str) -> Option<&'a str> {
        variant
            .get("properties")
            .and_then(|p| p.get(tag_property))
            .and_then(|p| p.get("const"))
            .and_then(|c| c.as_str())
    }

    /// Extract a `$ref` path from a oneOf variant schema.
    fn find_ref(variant: &Value) -> Option<&str> {
        variant.get("$ref").and_then(|r| r.as_str())
    }
}

impl schemars::transform::Transform for AddDiscriminator {
    fn transform(&mut self, schema: &mut schemars::Schema) {
        let Some(obj) = schema.as_object_mut() else {
            return;
        };
        let Some(one_of) = obj.get("oneOf").and_then(|v| v.as_array()) else {
            return;
        };

        // Build mapping from discriminator values to $ref paths
        let mut mapping = Map::new();
        for variant in one_of {
            if let Some(tag_value) = Self::find_tag_const(variant, &self.property_name)
                && let Some(ref_path) = Self::find_ref(variant)
            {
                mapping.insert(tag_value.to_string(), Value::String(ref_path.to_string()));
            }
        }

        // Build the discriminator object
        let mut discriminator = Map::new();
        discriminator.insert(
            "propertyName".to_string(),
            Value::String(self.property_name.clone()),
        );
        if !mapping.is_empty() {
            discriminator.insert("mapping".to_string(), Value::Object(mapping));
        }

        obj.insert("discriminator".to_string(), Value::Object(discriminator));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    // ---- Test 1: Tagged enum generates oneOf with const tag values ----

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "type", rename_all = "camelCase")]
    enum SimpleTaggedEnum {
        UnitVariant,
        DataVariant { value: String, count: i32 },
    }

    #[test]
    fn tagged_enum_has_one_of_with_const_tags() {
        let schema = schemars::schema_for!(SimpleTaggedEnum);
        let value = serde_json::to_value(&schema).unwrap();

        // Should have oneOf
        let one_of = value.get("oneOf").expect("should have oneOf");
        let variants = one_of.as_array().unwrap();
        assert_eq!(variants.len(), 2);

        // Each variant should have the tag property with a const value
        for variant in variants {
            let tag = variant
                .get("properties")
                .and_then(|p| p.get("type"))
                .expect("variant should have 'type' property");
            assert!(tag.get("const").is_some(), "tag should have const value");
        }

        insta::assert_yaml_snapshot!("tagged_enum_schema", value);
    }

    // ---- Test 2: AddDiscriminator transform adds discriminator ----

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "type", rename_all = "camelCase")]
    #[schemars(transform = AddDiscriminator::new("type"))]
    enum DiscriminatedEnum {
        UnitVariant,
        DataVariant { value: String },
    }

    #[test]
    fn add_discriminator_via_attribute() {
        let schema = schemars::schema_for!(DiscriminatedEnum);
        let value = serde_json::to_value(&schema).unwrap();

        // Should have discriminator with propertyName
        let discriminator = value
            .get("discriminator")
            .expect("should have discriminator");
        assert_eq!(
            discriminator.get("propertyName"),
            Some(&Value::String("type".to_string()))
        );

        insta::assert_yaml_snapshot!("discriminated_enum_schema", value);
    }

    // ---- Test 3: Variant titles for Python codegen ----

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "action", rename_all = "camelCase")]
    #[schemars(transform = AddDiscriminator::new("action"))]
    enum TitledVariantsEnum {
        #[schemars(title = "Fail")]
        Fail,
        #[schemars(title = "UseDefault")]
        UseDefault { default_value: serde_json::Value },
        #[schemars(title = "Retry")]
        Retry,
    }

    #[test]
    fn variant_titles_for_codegen() {
        let schema = schemars::schema_for!(TitledVariantsEnum);
        let value = serde_json::to_value(&schema).unwrap();

        // Each oneOf variant should have a title
        let variants = value
            .get("oneOf")
            .and_then(|v| v.as_array())
            .expect("should have oneOf array");
        for variant in variants {
            assert!(
                variant.get("title").is_some(),
                "variant should have title: {variant:?}"
            );
        }

        insta::assert_yaml_snapshot!("titled_variants_schema", value);
    }

    // ---- Test 4: Option/nullable types ----

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct NullableFields {
        required_field: String,
        optional_field: Option<String>,
        optional_complex: Option<Vec<i32>>,
    }

    #[test]
    fn nullable_types() {
        let schema = schemars::schema_for!(NullableFields);
        let value = serde_json::to_value(&schema).unwrap();

        // required_field should be in required list
        let required = value.get("required").and_then(|r| r.as_array()).unwrap();
        assert!(required.contains(&Value::String("requiredField".to_string())));

        // optional fields should NOT be in required
        assert!(!required.contains(&Value::String("optionalField".to_string())));

        insta::assert_yaml_snapshot!("nullable_fields_schema", value);
    }

    // ---- Test 5: Manual JsonSchema impl (ValueExpr-like pattern) ----
    // Validates we can write manual impls for types with custom serde.

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum SimpleExpr {
        Number(f64),
        Text(String),
        Bool(bool),
        Null,
    }

    impl JsonSchema for SimpleExpr {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "SimpleExpr".into()
        }

        fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
            schemars::json_schema!({
                "oneOf": [
                    { "type": "number", "title": "Number" },
                    { "type": "string", "title": "Text" },
                    { "type": "boolean", "title": "Bool" },
                    { "type": "null", "title": "Null" }
                ]
            })
        }
    }

    #[test]
    fn manual_json_schema_impl() {
        let schema = schemars::schema_for!(SimpleExpr);
        let value = serde_json::to_value(&schema).unwrap();

        assert!(value.get("oneOf").is_some(), "should have oneOf");

        insta::assert_yaml_snapshot!("manual_json_schema", value);
    }

    // ---- Test 6: Manual JsonSchema with discriminator (FlowResult pattern) ----
    // Validates manual impls with embedded discriminator for custom-serde types.

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "outcome")]
    enum CustomTaggedResult {
        #[serde(rename = "success")]
        Success { value: serde_json::Value },
        #[serde(rename = "failure")]
        Failure { error: String },
    }

    impl JsonSchema for CustomTaggedResult {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "CustomTaggedResult".into()
        }

        fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
            schemars::json_schema!({
                "oneOf": [
                    {
                        "title": "Success",
                        "type": "object",
                        "properties": {
                            "outcome": { "type": "string", "const": "success" },
                            "value": {}
                        },
                        "required": ["outcome", "value"]
                    },
                    {
                        "title": "Failure",
                        "type": "object",
                        "properties": {
                            "outcome": { "type": "string", "const": "failure" },
                            "error": { "type": "string" }
                        },
                        "required": ["outcome", "error"]
                    }
                ],
                "discriminator": {
                    "propertyName": "outcome"
                }
            })
        }
    }

    #[test]
    fn manual_discriminator_in_json_schema() {
        let schema = schemars::schema_for!(CustomTaggedResult);
        let value = serde_json::to_value(&schema).unwrap();

        let discriminator = value
            .get("discriminator")
            .expect("should have discriminator");
        assert_eq!(
            discriminator.get("propertyName"),
            Some(&Value::String("outcome".to_string()))
        );

        insta::assert_yaml_snapshot!("custom_tagged_result_schema", value);
    }

    // ---- Test 7: Tagged enum with newtype variant (references another type) ----
    // Validates schema output for enums like SupportedPlugin/LeaseManagerConfig
    // that wrap structs in their variants.

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase")]
    struct InnerConfig {
        endpoint: String,
        timeout_ms: u64,
    }

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "type", rename_all = "camelCase")]
    #[schemars(transform = AddDiscriminator::new("type"))]
    enum PluginConfig {
        #[schemars(title = "NoOp")]
        NoOp,
        #[schemars(title = "Remote")]
        Remote(InnerConfig),
    }

    #[test]
    fn tagged_enum_with_newtype_variant() {
        let schema = schemars::schema_for!(PluginConfig);
        let value = serde_json::to_value(&schema).unwrap();

        // Should have discriminator
        let discriminator = value
            .get("discriminator")
            .expect("should have discriminator");
        assert_eq!(
            discriminator.get("propertyName"),
            Some(&Value::String("type".to_string()))
        );

        insta::assert_yaml_snapshot!("plugin_config_schema", value);
    }

    // ---- Test 8: Full pipeline (inline → extract → discriminator mapping) ----

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "action", rename_all = "camelCase")]
    #[schemars(transform = AddDiscriminator::new("action"))]
    enum PipelineEnum {
        #[schemars(title = "Fail")]
        Fail,
        #[schemars(title = "UseDefault")]
        UseDefault { default_value: serde_json::Value },
        #[schemars(title = "Retry")]
        Retry,
    }

    #[test]
    fn full_pipeline_extracts_and_maps() {
        // generate_json_schema_with_defs runs finalize_discriminators which
        // extracts inline variants to $defs and builds discriminator mappings.
        let value = crate::json_schema::generate_json_schema_with_defs::<PipelineEnum>();

        // All oneOf entries should be $ref after extraction
        let one_of = value
            .get("oneOf")
            .and_then(|v| v.as_array())
            .expect("should have oneOf");
        for variant in one_of {
            assert!(
                variant.get("$ref").is_some(),
                "variant should be $ref after extraction, got: {variant}"
            );
        }

        // $defs keys should match variant titles
        let defs = value
            .get("$defs")
            .and_then(|v| v.as_object())
            .expect("should have $defs");
        assert!(defs.contains_key("Fail"), "$defs should have 'Fail'");
        assert!(
            defs.contains_key("UseDefault"),
            "$defs should have 'UseDefault'"
        );
        assert!(defs.contains_key("Retry"), "$defs should have 'Retry'");

        // Discriminator should have complete mapping
        let disc = value
            .get("discriminator")
            .and_then(|d| d.as_object())
            .expect("should have discriminator");
        let mapping = disc
            .get("mapping")
            .and_then(|m| m.as_object())
            .expect("discriminator should have mapping");
        assert_eq!(
            mapping.get("fail"),
            Some(&Value::String("#/$defs/Fail".to_string()))
        );
        assert_eq!(
            mapping.get("useDefault"),
            Some(&Value::String("#/$defs/UseDefault".to_string()))
        );
        assert_eq!(
            mapping.get("retry"),
            Some(&Value::String("#/$defs/Retry".to_string()))
        );

        insta::assert_yaml_snapshot!("full_pipeline_schema", value);
    }
}
