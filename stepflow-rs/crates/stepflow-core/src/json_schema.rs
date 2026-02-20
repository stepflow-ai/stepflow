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

//! Utility for generating standalone JSON Schema documents from schemars::JsonSchema types.
//!
//! This module provides functionality to generate standalone JSON Schema draft 2020-12
//! documents suitable for code generation tools like datamodel-code-generator.

use serde_json::Value;

use crate::discriminator_schema::AddDiscriminator;

/// Controls how external type references are handled in the generated schema.
#[derive(Debug, Clone)]
pub enum Refs {
    /// Omit external schemas - just reference them by name without definitions.
    /// Produces compact schemas suitable for documentation.
    Omit,
    /// Include external schemas in `$defs` with local references (`#/$defs/TypeName`).
    /// Produces self-contained schemas for validation.
    Local,
    /// Reference external schemas from an external URL.
    /// References become `{base_url}#/$defs/TypeName`.
    External(String),
}

/// Generate a standalone JSON Schema document from a type implementing JsonSchema.
///
/// This function generates a compact schema without `$defs` - any referenced types
/// will appear as `$ref` without definitions. This is suitable for component schemas
/// used for documentation purposes.
///
/// For a complete schema with all `$defs` included, use [`generate_json_schema_with_defs`].
pub fn generate_json_schema<T: schemars::JsonSchema>() -> Value {
    generate_json_schema_with_refs::<T>(Refs::Omit)
}

/// Generate a standalone JSON Schema document with all `$defs` included.
///
/// This function:
/// 1. Uses schemars to generate a complete JSON Schema for the type
/// 2. Includes all referenced schemas in `$defs`
/// 3. Applies AddDiscriminator transforms for tagged enums
///
/// Use this when you need a fully self-contained schema that can be validated
/// without external references.
pub fn generate_json_schema_with_defs<T: schemars::JsonSchema>() -> Value {
    generate_json_schema_with_refs::<T>(Refs::Local)
}

/// Generate a JSON Schema document with configurable reference handling.
///
/// # Arguments
/// * `refs` - Controls how external type references are handled:
///   - `Refs::Omit` - Omit `$defs`, just reference by name
///   - `Refs::Local` - Include schemas in `$defs` with local references
///   - `Refs::External(url)` - Reference schemas from an external URL
pub fn generate_json_schema_with_refs<T: schemars::JsonSchema>(refs: Refs) -> Value {
    generate_json_schema_custom::<T>(refs, |_| {})
}

/// Generate a JSON Schema document with configurable reference handling and
/// additional types seeded into `$defs`.
///
/// The `seed` callback receives a `&mut SchemaGenerator` before the root schema
/// is finalised.  Calling `generator.subschema_for::<ExtraType>()` inside the
/// callback ensures the type (and all its transitive deps) appear in `$defs`
/// even when they are not reachable from the root type `T`.
pub fn generate_json_schema_custom<T: schemars::JsonSchema>(
    refs: Refs,
    seed: impl FnOnce(&mut schemars::SchemaGenerator),
) -> Value {
    let settings = schemars::generate::SchemaSettings::draft2020_12();
    let mut generator = settings.into_generator();
    seed(&mut generator);
    let schema = generator.into_root_schema_for::<T>();
    let mut json = serde_json::to_value(schema).expect("Failed to serialize schema");

    match refs {
        Refs::Omit => {
            // Remove $defs entirely
            if let Some(obj) = json.as_object_mut() {
                obj.remove("$defs");
            }
        }
        Refs::Local => {
            // $defs are already using local references - nothing to change
        }
        Refs::External(ref base_url) => {
            // Transform #/$defs/X references to {base_url}#/$defs/X
            transform_refs_external(&mut json, base_url);
        }
    }

    // Apply AddDiscriminator transform to any oneOf schemas that have
    // inline const tag properties (indicating serde tagged enums)
    apply_discriminators(&mut json);

    // Extract inline discriminated variants into $defs.  Code generators like
    // datamodel-code-generator produce invalid code when a variant is inlined
    // (the tag field declared via `tag_field=` clashes with the `const` property).
    // Moving them to $defs and referencing via $ref avoids this.
    if !matches!(refs, Refs::Omit) {
        extract_discriminated_variants(&mut json);
    }

    // Convert oneOf-of-string-consts to enum form.  schemars generates
    // `oneOf: [{const: "a"}, {const: "b"}]` for simple string enums, but
    // code generators like datamodel-code-generator expect `enum: ["a", "b"]`.
    simplify_string_enums(&mut json);

    json
}

/// Recursively transform `#/$defs/X` references to `{base_url}#/$defs/X`.
fn transform_refs_external(value: &mut Value, base_url: &str) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(ref_str)) = map.get_mut("$ref")
                && let Some(name) = ref_str.strip_prefix("#/$defs/")
            {
                *ref_str = format!("{base_url}#/$defs/{name}");
            }
            for v in map.values_mut() {
                transform_refs_external(v, base_url);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                transform_refs_external(v, base_url);
            }
        }
        _ => {}
    }
}

/// Apply AddDiscriminator transforms to oneOf schemas that have
/// inline const tag properties (from serde tagged enums).
///
/// We detect these by looking for oneOf arrays where variants have
/// a "properties" object containing a field with a "const" value.
fn apply_discriminators(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Check if this object has a oneOf with const discriminators
            if let Some(Value::Array(one_of)) = map.get("oneOf")
                && let Some(tag_name) = detect_tag_property(one_of)
            {
                // Apply the discriminator transform
                let mut schema = schemars::Schema::try_from(Value::Object(map.clone())).unwrap();
                let mut transform = AddDiscriminator::new(tag_name);
                schemars::transform::Transform::transform(&mut transform, &mut schema);
                let transformed = serde_json::to_value(schema).expect("Failed to serialize schema");
                if let Value::Object(new_map) = transformed {
                    *map = new_map;
                }
            }

            // Recurse into all values (including $defs)
            for v in map.values_mut() {
                apply_discriminators(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                apply_discriminators(v);
            }
        }
        _ => {}
    }
}

/// Extract inline oneOf variants that have a discriminator into top-level `$defs`.
///
/// Code generators like datamodel-code-generator produce a `tag_field='type'`
/// parameter when they see a discriminator, but if the variant is inlined and
/// already contains a `type` property with a `const` value, the tag field
/// conflicts with the existing field.  By extracting titled variants into
/// `$defs` and referencing them via `$ref`, code generators handle them
/// correctly as named types.
fn extract_discriminated_variants(root: &mut Value) {
    // Collect all inline variants that need extraction
    let mut extracted: serde_json::Map<String, Value> = serde_json::Map::new();
    collect_inline_variants(root, &mut extracted);

    if extracted.is_empty() {
        return;
    }

    // Merge extracted variants into the root $defs
    if let Some(defs) = root
        .as_object_mut()
        .and_then(|obj| obj.get_mut("$defs"))
        .and_then(|d| d.as_object_mut())
    {
        for (name, schema) in extracted {
            defs.entry(name).or_insert(schema);
        }
    }
}

/// Walk the schema tree, collecting inline variants from discriminated oneOf
/// schemas.  Each collected variant is replaced in-place with a `$ref`.
///
/// The const tag property (e.g. `"type": {"const": "builtin"}`) is stripped
/// from extracted variants because code generators handle the tag via the
/// discriminator, and having it as a field causes conflicts.
fn collect_inline_variants(value: &mut Value, extracted: &mut serde_json::Map<String, Value>) {
    match value {
        Value::Object(map) => {
            // Check if this object has a oneOf with a discriminator
            let tag_property = map
                .get("discriminator")
                .and_then(|d| d.get("propertyName"))
                .and_then(|p| p.as_str())
                .map(|s| s.to_string());

            if let Some(ref tag_prop) = tag_property {
                let mut did_extract = false;
                if let Some(Value::Array(one_of)) = map.get_mut("oneOf") {
                    for variant in one_of.iter_mut() {
                        // Skip pure $ref variants (only have "$ref" key)
                        let is_pure_ref = variant
                            .as_object()
                            .is_some_and(|o| o.len() == 1 && o.contains_key("$ref"));
                        if is_pure_ref {
                            continue;
                        }
                        if let Some(title) = variant
                            .as_object()
                            .and_then(|v| v.get("title"))
                            .and_then(|t| t.as_str())
                        {
                            let name = title.to_string();
                            let mut schema = variant.clone();

                            // Strip the const tag property from the variant so
                            // code generators don't conflict with the discriminator
                            strip_tag_property(&mut schema, tag_prop);

                            extracted.insert(name.clone(), schema);
                            *variant = serde_json::json!({ "$ref": format!("#/$defs/{name}") });
                            did_extract = true;
                        }
                    }
                }
                // Remove the discriminator mapping since extracted $refs have
                // changed paths.  The mapping is optional per OpenAPI spec and
                // code generators can infer it from $ref paths.
                if did_extract && let Some(Value::Object(disc)) = map.get_mut("discriminator") {
                    disc.remove("mapping");
                }
            }

            // Recurse into all values
            for v in map.values_mut() {
                collect_inline_variants(v, extracted);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                collect_inline_variants(v, extracted);
            }
        }
        _ => {}
    }
}

/// Remove the discriminator tag property from a variant schema.
///
/// Removes the tag from `properties` and from `required`.
fn strip_tag_property(schema: &mut Value, tag_prop: &str) {
    if let Some(obj) = schema.as_object_mut() {
        if let Some(Value::Object(props)) = obj.get_mut("properties") {
            props.remove(tag_prop);
        }
        if let Some(Value::Array(required)) = obj.get_mut("required") {
            required.retain(|v| v.as_str() != Some(tag_prop));
        }
    }
}

/// Detect if a oneOf array represents a tagged enum by checking if all
/// variants have a common property with a "const" value.
fn detect_tag_property(one_of: &[Value]) -> Option<String> {
    // Skip if empty or only one variant
    if one_of.len() < 2 {
        return None;
    }

    // For each variant, find properties that have a "const" value
    let mut candidate_tags: Option<Vec<String>> = None;

    for variant in one_of {
        let props = variant
            .as_object()
            .and_then(|obj| obj.get("properties"))
            .and_then(|p| p.as_object());

        let Some(props) = props else {
            return None; // Not all variants have properties - not a tagged enum
        };

        let const_props: Vec<String> = props
            .iter()
            .filter(|(_, v)| v.get("const").is_some())
            .map(|(k, _)| k.clone())
            .collect();

        if const_props.is_empty() {
            return None; // No const properties in this variant
        }

        match &mut candidate_tags {
            None => candidate_tags = Some(const_props),
            Some(candidates) => {
                // Intersect with const props from this variant
                candidates.retain(|c| const_props.contains(c));
                if candidates.is_empty() {
                    return None; // No common tag property
                }
            }
        }
    }

    candidate_tags.and_then(|tags| tags.into_iter().next())
}

/// Convert `oneOf` arrays of string `const` values to `enum` form.
///
/// schemars generates `oneOf: [{type:"string", const:"a"}, {type:"string", const:"b"}]`
/// for simple string enums, but code generators expect `{type:"string", enum:["a","b"]}`.
fn simplify_string_enums(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(one_of)) = map.get("oneOf") {
                // Check if all variants are simple string const values
                let consts: Vec<&str> = one_of
                    .iter()
                    .filter_map(|v| {
                        let obj = v.as_object()?;
                        // Must be a simple const string (type: string, const: "...")
                        // with optionally a description
                        let is_string = obj
                            .get("type")
                            .and_then(|t| t.as_str())
                            .is_some_and(|t| t == "string");
                        let const_val = obj.get("const").and_then(|c| c.as_str());
                        let only_simple_keys = obj
                            .keys()
                            .all(|k| matches!(k.as_str(), "type" | "const" | "description"));
                        if is_string && only_simple_keys {
                            const_val
                        } else {
                            None
                        }
                    })
                    .collect();

                if consts.len() == one_of.len() && !consts.is_empty() {
                    // Replace oneOf with enum
                    let enum_values: Vec<Value> = consts
                        .iter()
                        .map(|c| Value::String(c.to_string()))
                        .collect();
                    map.remove("oneOf");
                    map.insert("type".to_string(), Value::String("string".to_string()));
                    map.insert("enum".to_string(), Value::Array(enum_values));
                }
            }

            // Recurse into all values
            for v in map.values_mut() {
                simplify_string_enums(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                simplify_string_enums(v);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_json_schema_has_required_fields() {
        use crate::blob::BlobId;

        let schema = generate_json_schema::<BlobId>();

        // Should have $schema declaration
        assert_eq!(
            schema.get("$schema"),
            Some(&Value::String(
                "https://json-schema.org/draft/2020-12/schema".to_string()
            ))
        );

        // Should have title
        assert!(schema.get("title").is_some());
    }

    #[test]
    fn test_generate_json_schema_with_defs() {
        use crate::workflow::Flow;

        let schema = generate_json_schema_with_defs::<Flow>();

        // Should have $schema
        assert!(schema.get("$schema").is_some());
        // Should have title
        assert!(schema.get("title").is_some());
        // Should have $defs
        assert!(schema.get("$defs").is_some());
    }

    #[test]
    fn test_transform_refs_external() {
        let mut input = serde_json::json!({
            "$ref": "#/$defs/MyType",
            "nested": {
                "$ref": "#/$defs/OtherType"
            },
            "array": [
                { "$ref": "#/$defs/ArrayItem" }
            ]
        });

        transform_refs_external(&mut input, "https://stepflow.org/schemas/v1/flow.json");

        assert_eq!(
            input,
            serde_json::json!({
                "$ref": "https://stepflow.org/schemas/v1/flow.json#/$defs/MyType",
                "nested": {
                    "$ref": "https://stepflow.org/schemas/v1/flow.json#/$defs/OtherType"
                },
                "array": [
                    { "$ref": "https://stepflow.org/schemas/v1/flow.json#/$defs/ArrayItem" }
                ]
            })
        );
    }

    #[test]
    fn test_detect_tag_property() {
        // Tagged enum variants with "type" discriminator
        let one_of = vec![
            serde_json::json!({
                "type": "object",
                "properties": {
                    "type": { "type": "string", "const": "variant_a" },
                    "value": { "type": "string" }
                }
            }),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "type": { "type": "string", "const": "variant_b" },
                    "count": { "type": "integer" }
                }
            }),
        ];

        assert_eq!(detect_tag_property(&one_of), Some("type".to_string()));
    }

    #[test]
    fn test_detect_tag_property_no_const() {
        // No const values - not a tagged enum
        let one_of = vec![
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                }
            }),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer" }
                }
            }),
        ];

        assert_eq!(detect_tag_property(&one_of), None);
    }
}
