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
/// This produces a fully self-contained schema suitable for validation
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
            finalize_discriminators(&mut json);
        }
        Refs::External(ref base_url) => {
            finalize_discriminators(&mut json);
            // Transform #/$defs/X references to {base_url}#/$defs/X
            transform_refs_external(&mut json, base_url);
        }
    }

    json
}

/// Post-process a generated schema to make discriminated unions work correctly
/// with code generators like `datamodel-code-generator`.
///
/// This runs three steps in order:
/// 1. **Extract inline `oneOf` variants** to the definitions section — variants
///    are keyed by their `title` attribute, so code generators produce the
///    expected type names.
/// 2. **Build discriminator mappings** by resolving `$ref` → definitions to read
///    tag `const` values and populate `discriminator.mapping`.
/// 3. **Add `default` alongside `const`** for discriminator tag properties —
///    `datamodel-code-generator` uses `default` (not `const`) to set tag values.
///
/// Schemas are resolved using `#/$defs/` references. For OpenAPI documents
/// where schemas live under `#/components/schemas/`, use
/// [`finalize_discriminators_with_prefix`].
pub fn finalize_discriminators(root: &mut Value) {
    finalize_discriminators_with_prefix(root, "#/$defs/");
}

/// Like [`finalize_discriminators`], but with a configurable `$ref` prefix.
///
/// The `ref_prefix` determines both where definitions are stored in the JSON
/// tree and the `$ref` prefix used in references:
/// - `"#/$defs/"` — JSON Schema (definitions at `root.$defs`)
/// - `"#/components/schemas/"` — OpenAPI (definitions at `root.components.schemas`)
pub fn finalize_discriminators_with_prefix(root: &mut Value, ref_prefix: &str) {
    extract_inline_oneof_to_defs(root, ref_prefix);
    build_discriminator_mappings(root, ref_prefix);
    add_defaults_to_discriminator_consts(root, ref_prefix);
}

/// Derive a JSON pointer path from a `$ref` prefix.
///
/// - `"#/$defs/"` → `"/$defs"`
/// - `"#/components/schemas/"` → `"/components/schemas"`
fn defs_pointer(ref_prefix: &str) -> &str {
    ref_prefix
        .strip_prefix('#')
        .unwrap_or(ref_prefix)
        .strip_suffix('/')
        .unwrap_or(ref_prefix)
}

/// Navigate to (and create if needed) the definitions object at the path
/// implied by `ref_prefix`.
fn get_or_create_defs_mut<'a>(
    root: &'a mut Value,
    ref_prefix: &str,
) -> &'a mut serde_json::Map<String, Value> {
    let pointer = defs_pointer(ref_prefix);
    let mut current = root;
    for segment in pointer.split('/').filter(|s| !s.is_empty()) {
        current = current
            .as_object_mut()
            .unwrap()
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    current.as_object_mut().unwrap()
}

/// Extract inline oneOf variants to the definitions section in schemas with
/// discriminators.
///
/// schemars inlines all variants in the `oneOf` array. Discriminator mappings
/// require `$ref` paths, so this extracts inline variants to definitions (using
/// their `title` as the key) and replaces them with `$ref` entries.
fn extract_inline_oneof_to_defs(root: &mut Value, ref_prefix: &str) {
    let mut extractions: Vec<(String, Value)> = Vec::new();
    extract_inline_oneof_recursive(root, ref_prefix, &mut extractions);

    if extractions.is_empty() {
        return;
    }

    let defs = get_or_create_defs_mut(root, ref_prefix);

    for (key, schema) in extractions {
        if let Some(existing) = defs.get_mut(&key) {
            // Collision: the variant's title matches an existing $defs key (the inner
            // type). Merge the discriminator tag property into the existing entry so
            // that code generators can read the tag const value.
            merge_tag_properties(existing, &schema);
        } else {
            defs.insert(key, schema);
        }
    }
}

fn extract_inline_oneof_recursive(
    value: &mut Value,
    ref_prefix: &str,
    extractions: &mut Vec<(String, Value)>,
) {
    match value {
        Value::Object(obj) => {
            if obj.contains_key("discriminator") {
                if let Some(Value::Array(one_of)) = obj.get_mut("oneOf") {
                    for variant in one_of.iter_mut() {
                        // Skip variants that are already pure $ref entries
                        if variant
                            .as_object()
                            .is_some_and(|o| o.len() == 1 && o.contains_key("$ref"))
                        {
                            continue;
                        }
                        // Extract inline variants with titles to $defs
                        if let Some(title) = variant
                            .get("title")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                        {
                            extractions.push((title.clone(), variant.clone()));
                            *variant =
                                serde_json::json!({ "$ref": format!("{ref_prefix}{title}") });
                        }
                    }
                }
            }

            for v in obj.values_mut() {
                extract_inline_oneof_recursive(v, ref_prefix, extractions);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                extract_inline_oneof_recursive(v, ref_prefix, extractions);
            }
        }
        _ => {}
    }
}

/// Merge discriminator tag properties from an extracted variant into an existing `$defs` entry.
///
/// When a variant's title matches an existing `$defs` key (e.g., `StepflowPluginConfig`
/// is both the inner type and the variant title), this adds the tag `const` property
/// and updates `required` so code generators can resolve the discriminator tag value.
fn merge_tag_properties(existing: &mut Value, variant: &Value) {
    // Merge properties (adds tag property from variant)
    if let Some(variant_props) = variant.get("properties").and_then(|p| p.as_object()) {
        let def_props = existing
            .as_object_mut()
            .unwrap()
            .entry("properties")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .unwrap();
        for (key, value) in variant_props {
            def_props.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }

    // Merge required arrays
    if let Some(variant_required) = variant.get("required").and_then(|r| r.as_array()) {
        let def_required = existing
            .as_object_mut()
            .unwrap()
            .entry("required")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .unwrap();
        for req in variant_required {
            if !def_required.contains(req) {
                def_required.push(req.clone());
            }
        }
    }
}

/// Build discriminator mappings by resolving `$ref` → definition entries
/// and reading tag `const` values.
fn build_discriminator_mappings(root: &mut Value, ref_prefix: &str) {
    let defs = root
        .pointer(defs_pointer(ref_prefix))
        .and_then(|v| v.as_object())
        .cloned();

    // Recursively process all schemas in the document
    build_discriminator_mappings_recursive(root, ref_prefix, defs.as_ref());
}

fn build_discriminator_mappings_recursive(
    value: &mut Value,
    ref_prefix: &str,
    defs: Option<&serde_json::Map<String, Value>>,
) {
    let Some(defs) = defs else { return };
    match value {
        Value::Object(obj) => {
            // Check if this object has a discriminator that needs mapping completion
            let needs_mapping = obj
                .get("discriminator")
                .is_some_and(|d| d.get("propertyName").is_some());

            if needs_mapping {
                if let Some(property_name) = obj
                    .get("discriminator")
                    .and_then(|d| d.get("propertyName"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
                {
                    if let Some(one_of) = obj.get("oneOf").and_then(|v| v.as_array()) {
                        let mut mapping = serde_json::Map::new();

                        for variant in one_of {
                            // Resolve $ref to the definition entry
                            if let Some(ref_path) = variant
                                .get("$ref")
                                .and_then(|r| r.as_str())
                            {
                                if let Some(def_key) = ref_path.strip_prefix(ref_prefix) {
                                    if let Some(def_schema) = defs.get(def_key) {
                                        // Read the const value for the discriminator property
                                        if let Some(const_val) = def_schema
                                            .get("properties")
                                            .and_then(|p| p.get(&property_name))
                                            .and_then(|p| p.get("const"))
                                            .and_then(|c| c.as_str())
                                        {
                                            mapping.insert(
                                                const_val.to_string(),
                                                Value::String(ref_path.to_string()),
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        if !mapping.is_empty() {
                            if let Some(disc) =
                                obj.get_mut("discriminator").and_then(|d| d.as_object_mut())
                            {
                                // Replace the mapping entirely — the post-processing steps
                                // (extract_inline_oneof_to_defs) may have changed $ref paths
                                disc.insert("mapping".to_string(), Value::Object(mapping));
                            }
                        }
                    }
                }
            }

            // Recurse into all values
            for v in obj.values_mut() {
                build_discriminator_mappings_recursive(v, ref_prefix, Some(defs));
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                build_discriminator_mappings_recursive(v, ref_prefix, Some(defs));
            }
        }
        _ => {}
    }
}

/// Add `default` alongside `const` for discriminator tag properties in definitions.
///
/// Code generators like `datamodel-code-generator` use `default` (not `const`)
/// to determine tag values for generated tagged union types. This walks all
/// definition entries referenced by discriminator mappings and adds `default`
/// equal to `const` for the discriminator tag property.
fn add_defaults_to_discriminator_consts(root: &mut Value, ref_prefix: &str) {
    let Some(root_obj) = root.as_object() else {
        return;
    };

    // Collect (def_key, property_name) pairs from all discriminator mappings
    let mut targets: Vec<(String, String)> = Vec::new();
    collect_discriminator_targets(root_obj, ref_prefix, &mut targets);

    if targets.is_empty() {
        return;
    }

    // Apply defaults to the collected targets
    let pointer = defs_pointer(ref_prefix);
    let Some(defs) = root.pointer_mut(pointer).and_then(|d| d.as_object_mut()) else {
        return;
    };

    for (def_key, property_name) in targets {
        if let Some(def_schema) = defs.get_mut(&def_key) {
            if let Some(prop) = def_schema
                .get_mut("properties")
                .and_then(|p| p.get_mut(&property_name))
                .and_then(|p| p.as_object_mut())
            {
                if let Some(const_val) = prop.get("const").cloned() {
                    prop.entry("default").or_insert(const_val);
                }
            }
        }
    }
}

fn collect_discriminator_targets(
    value: &serde_json::Map<String, Value>,
    ref_prefix: &str,
    targets: &mut Vec<(String, String)>,
) {
    if let Some(disc) = value.get("discriminator").and_then(|d| d.as_object()) {
        if let Some(property_name) = disc.get("propertyName").and_then(|p| p.as_str()) {
            if let Some(mapping) = disc.get("mapping").and_then(|m| m.as_object()) {
                for ref_path in mapping.values() {
                    if let Some(ref_str) = ref_path.as_str() {
                        if let Some(def_key) = ref_str.strip_prefix(ref_prefix) {
                            targets.push((def_key.to_string(), property_name.to_string()));
                        }
                    }
                }
            }
        }
    }

    // Recurse into nested objects
    for v in value.values() {
        if let Some(obj) = v.as_object() {
            collect_discriminator_targets(obj, ref_prefix, targets);
        } else if let Some(arr) = v.as_array() {
            for item in arr {
                if let Some(obj) = item.as_object() {
                    collect_discriminator_targets(obj, ref_prefix, targets);
                }
            }
        }
    }
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

}
