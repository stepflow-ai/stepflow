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

//! Utility for generating standalone JSON Schema documents from utoipa::ToSchema types.
//!
//! This module provides functionality to convert utoipa's OpenAPI-style schemas into
//! standalone JSON Schema draft 2020-12 documents suitable for code generation tools
//! like datamodel-code-generator.

use serde_json::{Map, Value};
use utoipa::PartialSchema;
use utoipa::ToSchema;

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

/// Generate a standalone JSON Schema document from a type implementing ToSchema.
///
/// This function generates a compact schema without `$defs` - any referenced types
/// will appear as `$ref` without definitions. This is suitable for component schemas
/// used for documentation purposes.
///
/// For a complete schema with all `$defs` included, use [`generate_json_schema_with_defs`].
///
/// # Example
/// ```ignore
/// use stepflow_core::json_schema::generate_json_schema;
/// use stepflow_core::workflow::Flow;
///
/// let schema = generate_json_schema::<Flow>();
/// let json_str = serde_json::to_string_pretty(&schema).unwrap();
/// ```
pub fn generate_json_schema<T: ToSchema + PartialSchema>() -> Value {
    generate_json_schema_with_refs::<T>(Refs::Omit)
}

/// Generate a standalone JSON Schema document with all `$defs` included.
///
/// This function:
/// 1. Collects all schemas from the type and its dependencies
/// 2. Builds a JSON Schema document with the root type's schema
/// 3. Places all referenced schemas in `$defs`
/// 4. Transforms `#/components/schemas/X` references to `#/$defs/X`
///
/// Use this when you need a fully self-contained schema that can be validated
/// without external references.
pub fn generate_json_schema_with_defs<T: ToSchema + PartialSchema>() -> Value {
    generate_json_schema_with_refs::<T>(Refs::Local)
}

/// Generate a JSON Schema document with configurable reference handling.
///
/// # Arguments
/// * `refs` - Controls how external type references are handled:
///   - `Refs::Omit` - Omit `$defs`, just reference by name
///   - `Refs::Local` - Include schemas in `$defs` with local references
///   - `Refs::External(url)` - Reference schemas from an external URL
pub fn generate_json_schema_with_refs<T: ToSchema + PartialSchema>(refs: Refs) -> Value {
    generate_json_schema_impl::<T>(refs)
}

fn generate_json_schema_impl<T: ToSchema + PartialSchema>(refs: Refs) -> Value {
    // Collect all schemas from the type
    let mut schemas: Vec<(
        String,
        utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    )> = Vec::new();
    T::schemas(&mut schemas);

    // Get the root schema
    let root_schema = T::schema();
    let root_name = T::name();

    // Convert schemas to JSON and collect into $defs
    let mut defs: Map<String, Value> = Map::new();
    for (name, schema) in schemas {
        let json = serde_json::to_value(&schema).expect("Failed to serialize schema");
        let transformed = transform_refs(json, &refs);
        defs.insert(name, transformed);
    }

    // Build the root schema
    let root_json = serde_json::to_value(&root_schema).expect("Failed to serialize root schema");
    let mut root_transformed = transform_refs(root_json, &refs);

    // If root is a $ref, we need to inline the referenced schema at the top level
    // and keep it in $defs for other references
    if let Some(obj) = root_transformed.as_object_mut() {
        // Add $schema declaration
        let mut result: Map<String, Value> = Map::new();
        result.insert(
            "$schema".to_string(),
            Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
        );

        // Add title from the type name
        result.insert("title".to_string(), Value::String(root_name.to_string()));

        // If root is just a $ref, resolve it (only for Local refs where we have defs)
        if obj.len() == 1 && obj.contains_key("$ref") && matches!(refs, Refs::Local) {
            // Get the referenced schema name from #/$defs/Name
            if let Some(Value::String(ref_path)) = obj.get("$ref")
                && let Some(ref_name) = ref_path.strip_prefix("#/$defs/")
                && let Some(referenced_schema) = defs.get(ref_name)
            {
                // Copy properties from the referenced schema
                if let Some(ref_obj) = referenced_schema.as_object() {
                    for (key, value) in ref_obj {
                        if key != "title" {
                            // Keep our title
                            result.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        } else {
            // Copy all properties from root
            for (key, value) in obj.iter() {
                result.insert(key.clone(), value.clone());
            }
        }

        // Add $defs if Local refs and we have any
        if matches!(refs, Refs::Local) && !defs.is_empty() {
            result.insert("$defs".to_string(), Value::Object(defs.clone()));
        }

        let mut final_result = Value::Object(result);

        // Inline allOf patterns at root oneOf level for Python codegen compatibility
        if matches!(refs, Refs::Local) {
            inline_root_one_of(&mut final_result, &defs);
        }

        final_result
    } else {
        // Unexpected: root is not an object
        root_transformed
    }
}

/// Transform the schema to use JSON Schema conventions:
/// - Transform `#/components/schemas/X` references based on the Refs mode
/// - Convert single-item `enum` arrays to `const` values
/// - Handle discriminator `mapping` values
fn transform_refs(value: Value, refs: &Refs) -> Value {
    match value {
        Value::Object(mut map) => {
            // Check if this is a $ref
            if let Some(Value::String(ref_str)) = map.get("$ref")
                && let Some(name) = ref_str.strip_prefix("#/components/schemas/")
            {
                let new_ref = match refs {
                    Refs::Omit | Refs::Local => format!("#/$defs/{}", name),
                    Refs::External(base_url) => format!("{}#/$defs/{}", base_url, name),
                };
                map.insert("$ref".to_string(), Value::String(new_ref));
            }

            // Convert single-item enum to const
            // e.g., {"type": "string", "enum": ["value"]} -> {"type": "string", "const": "value"}
            if let Some(Value::Array(enum_values)) = map.get("enum")
                && enum_values.len() == 1
            {
                let const_value = enum_values[0].clone();
                map.remove("enum");
                map.insert("const".to_string(), const_value);
            }

            // Handle discriminator mapping values
            if let Some(discriminator) = map.get_mut("discriminator")
                && let Some(mapping) = discriminator.get_mut("mapping")
                && let Some(mapping_obj) = mapping.as_object_mut()
            {
                for (_, v) in mapping_obj.iter_mut() {
                    if let Some(ref_str) = v.as_str()
                        && let Some(name) = ref_str.strip_prefix("#/components/schemas/")
                    {
                        let new_ref = match refs {
                            Refs::Omit | Refs::Local => format!("#/$defs/{}", name),
                            Refs::External(base_url) => {
                                format!("{}#/$defs/{}", base_url, name)
                            }
                        };
                        *v = Value::String(new_ref);
                    }
                }
            }

            // Recursively transform all values
            let transformed: Map<String, Value> = map
                .into_iter()
                .map(|(k, v)| (k, transform_refs(v, refs)))
                .collect();
            Value::Object(transformed)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(|v| transform_refs(v, refs)).collect())
        }
        other => other,
    }
}

/// Inline allOf patterns at the root oneOf level to produce codegen-compatible output.
///
/// Transforms `oneOf: [{ allOf: [{ $ref: "#/$defs/X" }, { ...discriminator... }] }]`
/// into `oneOf: [{ title: "X", ...X's properties..., ...discriminator... }]`
fn inline_root_one_of(schema: &mut Value, defs: &Map<String, Value>) {
    if let Some(one_of) = schema.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
        for item in one_of.iter_mut() {
            if let Some(all_of) = item.get("allOf").and_then(|v| v.as_array()) {
                // Check if this is a pattern with $ref + discriminator object
                if all_of.len() == 2 {
                    let (ref_item, extra) = if all_of[0].get("$ref").is_some() {
                        (&all_of[0], &all_of[1])
                    } else if all_of[1].get("$ref").is_some() {
                        (&all_of[1], &all_of[0])
                    } else {
                        continue;
                    };

                    // Get the referenced schema
                    if let Some(Value::String(ref_path)) = ref_item.get("$ref")
                        && let Some(ref_name) = ref_path.strip_prefix("#/$defs/")
                        && let Some(referenced_schema) = defs.get(ref_name)
                    {
                        // Create a new inlined schema
                        let mut inlined = referenced_schema.clone();
                        if let Some(inlined_obj) = inlined.as_object_mut() {
                            // Add title if not present
                            if !inlined_obj.contains_key("title") {
                                inlined_obj.insert(
                                    "title".to_string(),
                                    Value::String(ref_name.to_string()),
                                );
                            }

                            // Merge extra properties (discriminator) into the inlined schema
                            if let Some(extra_obj) = extra.as_object() {
                                // Merge properties
                                if let Some(extra_props) =
                                    extra_obj.get("properties").and_then(|v| v.as_object())
                                {
                                    let props = inlined_obj
                                        .entry("properties")
                                        .or_insert_with(|| Value::Object(Map::new()));
                                    if let Some(props_obj) = props.as_object_mut() {
                                        for (k, v) in extra_props {
                                            props_obj.insert(k.clone(), v.clone());
                                        }
                                    }
                                }

                                // Merge required
                                if let Some(extra_required) =
                                    extra_obj.get("required").and_then(|v| v.as_array())
                                {
                                    let required = inlined_obj
                                        .entry("required")
                                        .or_insert_with(|| Value::Array(vec![]));
                                    if let Some(required_arr) = required.as_array_mut() {
                                        for req in extra_required {
                                            if !required_arr.contains(req) {
                                                required_arr.push(req.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Replace the allOf with the inlined schema
                        *item = inlined;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_refs_local() {
        let input = serde_json::json!({
            "$ref": "#/components/schemas/MyType",
            "nested": {
                "$ref": "#/components/schemas/OtherType"
            },
            "array": [
                { "$ref": "#/components/schemas/ArrayItem" }
            ]
        });

        let output = transform_refs(input, &Refs::Local);

        assert_eq!(
            output,
            serde_json::json!({
                "$ref": "#/$defs/MyType",
                "nested": {
                    "$ref": "#/$defs/OtherType"
                },
                "array": [
                    { "$ref": "#/$defs/ArrayItem" }
                ]
            })
        );
    }

    #[test]
    fn test_transform_refs_external() {
        let input = serde_json::json!({
            "$ref": "#/components/schemas/MyType",
            "nested": {
                "$ref": "#/components/schemas/OtherType"
            }
        });

        let output = transform_refs(
            input,
            &Refs::External("https://stepflow.org/schemas/v1/flow.json".to_string()),
        );

        assert_eq!(
            output,
            serde_json::json!({
                "$ref": "https://stepflow.org/schemas/v1/flow.json#/$defs/MyType",
                "nested": {
                    "$ref": "https://stepflow.org/schemas/v1/flow.json#/$defs/OtherType"
                }
            })
        );
    }

    #[test]
    fn test_generate_json_schema_has_required_fields() {
        // Test with a simple type that implements ToSchema
        // We'll use BlobId since it's in this crate and has ToSchema
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
}
