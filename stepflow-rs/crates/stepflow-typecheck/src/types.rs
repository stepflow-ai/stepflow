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

//! Type representation for the type checker.

use stepflow_core::schema::SchemaRef;

/// Represents a type in the Stepflow type system.
///
/// The type system is based on JSON Schema, extended with special types
/// for gradual typing support.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// A concrete JSON Schema type.
    ///
    /// Values of this type must validate against the given schema.
    Schema(SchemaRef),

    /// The "any" type - matches any value.
    ///
    /// Represented as an empty JSON Schema `{}`. Used when:
    /// - A component doesn't provide schema information
    /// - In permissive mode for untyped outputs
    ///
    /// Subtyping: `Any` is a supertype of everything, and checking
    /// against `Any` always succeeds. However, when `Any` appears as
    /// a subtype, the result is `Unknown` (cannot verify statically).
    Any,

    /// The "never" type - represents impossible/contradictory types.
    ///
    /// Used when type inference produces a contradiction. No values
    /// can have this type.
    ///
    /// Subtyping: `Never` is a subtype of everything (vacuously true).
    Never,
}

impl Type {
    /// Create a Type from an optional SchemaRef.
    ///
    /// Returns `Any` if the schema is `None`, otherwise wraps it in `Schema`.
    pub fn from_option(schema: Option<SchemaRef>) -> Self {
        match schema {
            Some(s) => Type::Schema(s),
            None => Type::Any,
        }
    }

    /// Check if this is the `Any` type.
    pub fn is_any(&self) -> bool {
        matches!(self, Type::Any)
    }

    /// Check if this is the `Never` type.
    pub fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    /// Check if this type is concrete (i.e., a Schema).
    pub fn is_concrete(&self) -> bool {
        matches!(self, Type::Schema(_))
    }

    /// Get the underlying schema, if this is a concrete type.
    pub fn as_schema(&self) -> Option<&SchemaRef> {
        match self {
            Type::Schema(s) => Some(s),
            _ => None,
        }
    }

    /// Convert to an optional SchemaRef, consuming self.
    ///
    /// Returns `Some(schema)` for `Type::Schema`, `None` for `Any` and `Never`.
    pub fn into_schema(self) -> Option<SchemaRef> {
        match self {
            Type::Schema(s) => Some(s),
            _ => None,
        }
    }

    /// Create an `Any` type, equivalent to empty JSON Schema `{}`.
    pub fn any() -> Self {
        Type::Any
    }

    /// Create a `Never` type.
    pub fn never() -> Self {
        Type::Never
    }

    /// Create a type from a JSON Schema value.
    pub fn from_json(value: serde_json::Value) -> Self {
        Type::Schema(SchemaRef::from(value))
    }
}

impl Default for Type {
    /// Default type is `Any`, representing no type constraints.
    fn default() -> Self {
        Type::Any
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Schema(s) => {
                let value = s.as_value();
                format_schema_type(f, value)
            }
            Type::Any => write!(f, "any"),
            Type::Never => write!(f, "never"),
        }
    }
}

/// Format a JSON Schema type in a concise, readable way.
fn format_schema_type(
    f: &mut std::fmt::Formatter<'_>,
    schema: &serde_json::Value,
) -> std::fmt::Result {
    format_schema_type_depth(f, schema, 0)
}

/// Format a JSON Schema type with depth tracking for nested objects.
fn format_schema_type_depth(
    f: &mut std::fmt::Formatter<'_>,
    schema: &serde_json::Value,
    depth: usize,
) -> std::fmt::Result {
    const MAX_DEPTH: usize = 3; // Show up to 3 levels of nesting

    // Handle empty schema (any)
    if schema.as_object().is_some_and(|o| o.is_empty()) {
        return write!(f, "any");
    }

    // Check for type field
    if let Some(ty) = schema.get("type").and_then(|t| t.as_str()) {
        match ty {
            "object" => {
                // Show object with property names
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    if depth >= MAX_DEPTH {
                        // Too deep, just show "object"
                        write!(f, "object")
                    } else {
                        let prop_strs: Vec<String> = props
                            .iter()
                            .map(|(name, prop_schema)| {
                                let secret_marker = if prop_schema
                                    .get("is_secret")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false)
                                {
                                    " [secret]"
                                } else {
                                    ""
                                };
                                let prop_type = get_type_at_depth(prop_schema, depth + 1);
                                format!("{}: {}{}", name, prop_type, secret_marker)
                            })
                            .collect();
                        write!(f, "{{ {} }}", prop_strs.join(", "))
                    }
                } else {
                    write!(f, "object")
                }
            }
            "array" => {
                // Show array with item type
                if let Some(items) = schema.get("items") {
                    let item_type = get_type_at_depth(items, depth + 1);
                    write!(f, "{}[]", item_type)
                } else {
                    write!(f, "array")
                }
            }
            _ => write!(f, "{}", ty),
        }
    } else if schema.get("anyOf").is_some() || schema.get("oneOf").is_some() {
        // Union type
        write!(f, "union")
    } else if let Some(ref_val) = schema.get("$ref").and_then(|r| r.as_str()) {
        // Extract the type name from the $ref path (e.g., "#/$defs/ChatMessage" -> "ChatMessage")
        let type_name = ref_val.rsplit('/').next().unwrap_or("ref");
        write!(f, "{}", type_name)
    } else {
        // Fall back to JSON for complex schemas
        write!(
            f,
            "{}",
            serde_json::to_string(schema).unwrap_or_else(|_| "<schema>".to_string())
        )
    }
}

/// Get a type name from a schema at a given depth.
fn get_type_at_depth(schema: &serde_json::Value, depth: usize) -> String {
    const MAX_DEPTH: usize = 3;

    if schema.as_object().is_some_and(|o| o.is_empty()) {
        return "any".to_string();
    }

    if let Some(ty) = schema.get("type").and_then(|t| t.as_str()) {
        match ty {
            "object" => {
                if depth >= MAX_DEPTH {
                    "object".to_string()
                } else if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    let prop_strs: Vec<String> = props
                        .iter()
                        .map(|(name, prop_schema)| {
                            let secret_marker = if prop_schema
                                .get("is_secret")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                            {
                                " [secret]"
                            } else {
                                ""
                            };
                            let prop_type = get_type_at_depth(prop_schema, depth + 1);
                            format!("{}: {}{}", name, prop_type, secret_marker)
                        })
                        .collect();
                    format!("{{ {} }}", prop_strs.join(", "))
                } else {
                    "object".to_string()
                }
            }
            "array" => {
                if let Some(items) = schema.get("items") {
                    format!("{}[]", get_type_at_depth(items, depth + 1))
                } else {
                    "array".to_string()
                }
            }
            _ => ty.to_string(),
        }
    } else if schema.get("anyOf").is_some() || schema.get("oneOf").is_some() {
        "union".to_string()
    } else if let Some(ref_val) = schema.get("$ref").and_then(|r| r.as_str()) {
        // Extract the type name from the $ref path (e.g., "#/$defs/ChatMessage" -> "ChatMessage")
        ref_val.rsplit('/').next().unwrap_or("ref").to_string()
    } else {
        "any".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_from_option_some() {
        let schema = SchemaRef::from(serde_json::json!({"type": "string"}));
        let ty = Type::from_option(Some(schema.clone()));
        assert!(ty.is_concrete());
        assert_eq!(ty.as_schema(), Some(&schema));
    }

    #[test]
    fn test_type_from_option_none() {
        let ty = Type::from_option(None);
        assert!(ty.is_any());
        assert!(!ty.is_concrete());
    }

    #[test]
    fn test_type_predicates() {
        assert!(Type::Any.is_any());
        assert!(!Type::Any.is_never());
        assert!(!Type::Any.is_concrete());

        assert!(!Type::Never.is_any());
        assert!(Type::Never.is_never());
        assert!(!Type::Never.is_concrete());

        let schema = Type::from_json(serde_json::json!({"type": "number"}));
        assert!(!schema.is_any());
        assert!(!schema.is_never());
        assert!(schema.is_concrete());
    }

    #[test]
    fn test_type_display() {
        assert_eq!(Type::Any.to_string(), "any");
        assert_eq!(Type::Never.to_string(), "never");

        let string_type = Type::from_json(serde_json::json!({"type": "string"}));
        assert_eq!(string_type.to_string(), "string");

        let number_type = Type::from_json(serde_json::json!({"type": "number"}));
        assert_eq!(number_type.to_string(), "number");
    }

    #[test]
    fn test_type_display_object_with_properties() {
        let obj_type = Type::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            }
        }));
        let display = obj_type.to_string();
        // Should show property names and types
        assert!(display.contains("name: string"), "display: {}", display);
        assert!(display.contains("age: number"), "display: {}", display);
    }

    #[test]
    fn test_type_display_object_with_secret() {
        let obj_type = Type::from_json(serde_json::json!({
            "type": "object",
            "properties": {
                "api_key": {"type": "string", "is_secret": true},
                "name": {"type": "string"}
            }
        }));
        let display = obj_type.to_string();
        // Should mark secret fields
        assert!(
            display.contains("api_key: string [secret]"),
            "display: {}",
            display
        );
        assert!(display.contains("name: string"), "display: {}", display);
        // name should not be marked as secret
        assert!(
            !display.contains("name: string [secret]"),
            "display: {}",
            display
        );
    }

    #[test]
    fn test_type_display_array() {
        let array_type = Type::from_json(serde_json::json!({
            "type": "array",
            "items": {"type": "number"}
        }));
        assert_eq!(array_type.to_string(), "number[]");
    }

    #[test]
    fn test_type_default() {
        let ty: Type = Default::default();
        assert!(ty.is_any());
    }
}
