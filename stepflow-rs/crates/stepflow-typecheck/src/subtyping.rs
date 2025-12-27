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

//! JSON Schema subtyping.
//!
//! This module implements structural subtyping for JSON Schema types.
//! A type `S` is a subtype of type `T` (written `S <: T`) if any value
//! that validates against `S` also validates against `T`.
//!
//! # Subtyping Rules
//!
//! - **Any**: `Any` is a supertype of everything. When checking `S <: Any`, the result is `Yes`.
//!   When checking `Any <: T`, the result is `Unknown` (cannot verify statically).
//! - **Never**: `Never` is a subtype of everything (vacuously true).
//! - **Width subtyping**: Objects with more properties are subtypes of objects with fewer.
//! - **Depth subtyping**: Property types must be subtypes of the supertype's property types.
//! - **Array covariance**: `T[]` is a subtype of `U[]` if `T <: U`.

use serde_json::Value;

use crate::Type;

/// Result of a subtype check.
#[derive(Debug, Clone, PartialEq)]
pub enum SubtypeResult {
    /// The subtype relationship holds.
    Yes,

    /// The subtype relationship does not hold, with reasons.
    No(Vec<String>),

    /// Cannot determine the relationship statically.
    ///
    /// This occurs when `Any` appears as a subtype, meaning we can't
    /// verify statically that all values will be compatible.
    Unknown,
}

impl SubtypeResult {
    /// Check if the result indicates a successful subtype relationship.
    pub fn is_yes(&self) -> bool {
        matches!(self, SubtypeResult::Yes)
    }

    /// Check if the result indicates a failed subtype relationship.
    pub fn is_no(&self) -> bool {
        matches!(self, SubtypeResult::No(_))
    }

    /// Check if the result is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, SubtypeResult::Unknown)
    }

    /// Get the reasons for failure, if any.
    pub fn reasons(&self) -> Option<&[String]> {
        match self {
            SubtypeResult::No(reasons) => Some(reasons),
            _ => None,
        }
    }

    /// Combine two subtype results.
    ///
    /// Used when checking multiple constraints. The result is:
    /// - `No` if either is `No` (accumulates reasons)
    /// - `Unknown` if either is `Unknown` (and neither is `No`)
    /// - `Yes` only if both are `Yes`
    pub fn and(self, other: SubtypeResult) -> SubtypeResult {
        match (self, other) {
            (SubtypeResult::No(mut a), SubtypeResult::No(b)) => {
                a.extend(b);
                SubtypeResult::No(a)
            }
            (SubtypeResult::No(a), _) | (_, SubtypeResult::No(a)) => SubtypeResult::No(a),
            (SubtypeResult::Unknown, _) | (_, SubtypeResult::Unknown) => SubtypeResult::Unknown,
            (SubtypeResult::Yes, SubtypeResult::Yes) => SubtypeResult::Yes,
        }
    }
}

/// Check if `subtype` is a subtype of `supertype`.
///
/// Returns `Yes` if any value that validates against `subtype` also validates
/// against `supertype`. Returns `No` with reasons if the relationship doesn't
/// hold. Returns `Unknown` if it cannot be determined statically.
pub fn is_subtype(subtype: &Type, supertype: &Type) -> SubtypeResult {
    match (subtype, supertype) {
        // Any is a supertype of everything
        (_, Type::Any) => SubtypeResult::Yes,

        // Any as subtype - cannot verify statically
        (Type::Any, _) => SubtypeResult::Unknown,

        // Never is a subtype of everything (vacuously true)
        (Type::Never, _) => SubtypeResult::Yes,

        // Nothing is a subtype of Never (except Never itself, handled above)
        (_, Type::Never) => SubtypeResult::No(vec!["cannot assign to never type".to_string()]),

        // Concrete schema comparison
        (Type::Schema(sub), Type::Schema(sup)) => {
            check_schema_subtype(sub.as_value(), sup.as_value())
        }
    }
}

/// Check if a JSON Schema is a subtype of another.
fn check_schema_subtype(sub: &Value, sup: &Value) -> SubtypeResult {
    // Empty schema {} means "any" - everything is a subtype
    if is_empty_schema(sup) {
        return SubtypeResult::Yes;
    }

    // Handle boolean schemas (true = any, false = never)
    match (sub.as_bool(), sup.as_bool()) {
        (_, Some(true)) => return SubtypeResult::Yes,
        (Some(false), _) => return SubtypeResult::Yes, // false <: anything
        (_, Some(false)) => {
            return SubtypeResult::No(vec!["cannot match 'false' schema".to_string()]);
        }
        _ => {}
    }

    // Get the types
    let sub_type = get_schema_type(sub);
    let sup_type = get_schema_type(sup);

    // Check for type compatibility
    match (&sub_type, &sup_type) {
        // Both have explicit types
        (Some(st), Some(spt)) if st != spt => {
            return SubtypeResult::No(vec![format!(
                "type mismatch: expected '{}', got '{}'",
                spt, st
            )]);
        }
        _ => {}
    }

    // Type-specific subtyping
    match sup_type.as_deref() {
        Some("object") => check_object_subtype(sub, sup),
        Some("array") => check_array_subtype(sub, sup),
        Some("string" | "number" | "integer" | "boolean" | "null") => {
            check_primitive_subtype(sub, sup)
        }
        None => {
            // No type specified - check composite schemas
            check_composite_subtype(sub, sup)
        }
        Some(unknown) => SubtypeResult::No(vec![format!("unknown type: '{}'", unknown)]),
    }
}

/// Check if a schema is the empty schema {}.
fn is_empty_schema(schema: &Value) -> bool {
    schema.as_object().is_some_and(|o| o.is_empty())
}

/// Get the "type" field from a schema.
fn get_schema_type(schema: &Value) -> Option<String> {
    schema
        .get("type")
        .and_then(|t| t.as_str())
        .map(String::from)
}

/// Check object subtyping.
///
/// An object type S is a subtype of T if:
/// - S has all required properties of T
/// - Each property type in S is a subtype of the corresponding property type in T
/// - If T disallows additional properties, S must not have extras
fn check_object_subtype(sub: &Value, sup: &Value) -> SubtypeResult {
    let mut result = SubtypeResult::Yes;

    // Check that subtype has required properties
    if let Some(required) = sup.get("required").and_then(|r| r.as_array()) {
        let sub_props = sub
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|o| {
                o.keys()
                    .map(|k| k.as_str())
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();

        for req in required {
            if let Some(prop_name) = req.as_str()
                && !sub_props.contains(prop_name)
            {
                result = result.and(SubtypeResult::No(vec![format!(
                    "missing required property: '{}'",
                    prop_name
                )]));
            }
        }
    }

    // Check property types match (depth subtyping)
    if let (Some(sub_props), Some(sup_props)) = (
        sub.get("properties").and_then(|p| p.as_object()),
        sup.get("properties").and_then(|p| p.as_object()),
    ) {
        for (name, sup_prop_schema) in sup_props {
            if let Some(sub_prop_schema) = sub_props.get(name) {
                let prop_result = check_schema_subtype(sub_prop_schema, sup_prop_schema);
                if let SubtypeResult::No(reasons) = prop_result {
                    let prefixed: Vec<_> = reasons
                        .into_iter()
                        .map(|r| format!("property '{}': {}", name, r))
                        .collect();
                    result = result.and(SubtypeResult::No(prefixed));
                } else {
                    result = result.and(prop_result);
                }
            }
        }
    }

    // Check additionalProperties constraint
    if sup.get("additionalProperties") == Some(&Value::Bool(false)) {
        let sub_props: std::collections::HashSet<&str> = sub
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|o| o.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();

        let sup_props: std::collections::HashSet<&str> = sup
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|o| o.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();

        for extra in sub_props.difference(&sup_props) {
            result = result.and(SubtypeResult::No(vec![format!(
                "unexpected property '{}' not allowed",
                extra
            )]));
        }
    }

    result
}

/// Check array subtyping.
///
/// Array type S[] is a subtype of T[] if S <: T (covariant).
fn check_array_subtype(sub: &Value, sup: &Value) -> SubtypeResult {
    // Check items schema
    match (sub.get("items"), sup.get("items")) {
        (Some(sub_items), Some(sup_items)) => check_schema_subtype(sub_items, sup_items),
        (None, Some(_)) => SubtypeResult::Unknown, // Sub doesn't constrain items
        _ => SubtypeResult::Yes,
    }
}

/// Check primitive type constraints.
fn check_primitive_subtype(sub: &Value, sup: &Value) -> SubtypeResult {
    let mut result = SubtypeResult::Yes;

    // Check enum constraints
    if let Some(sup_enum) = sup.get("enum").and_then(|e| e.as_array()) {
        if let Some(sub_enum) = sub.get("enum").and_then(|e| e.as_array()) {
            // All sub enum values must be in sup enum
            for val in sub_enum {
                if !sup_enum.contains(val) {
                    result = result.and(SubtypeResult::No(vec![format!(
                        "enum value {:?} not in allowed values",
                        val
                    )]));
                }
            }
        }
        // If sub doesn't have enum but sup does, we can't verify
        else {
            result = result.and(SubtypeResult::Unknown);
        }
    }

    // Check const constraints
    if let Some(sup_const) = sup.get("const") {
        if let Some(sub_const) = sub.get("const") {
            if sub_const != sup_const {
                result = result.and(SubtypeResult::No(vec![format!(
                    "const mismatch: expected {:?}, got {:?}",
                    sup_const, sub_const
                )]));
            }
        } else {
            result = result.and(SubtypeResult::Unknown);
        }
    }

    result
}

/// Check composite schema (oneOf, anyOf, allOf).
fn check_composite_subtype(sub: &Value, sup: &Value) -> SubtypeResult {
    // Handle oneOf: sub must be subtype of at least one option
    if let Some(one_of) = sup.get("oneOf").and_then(|o| o.as_array()) {
        for option in one_of {
            if check_schema_subtype(sub, option).is_yes() {
                return SubtypeResult::Yes;
            }
        }
        return SubtypeResult::No(vec!["does not match any oneOf option".to_string()]);
    }

    // Handle anyOf: sub must be subtype of at least one option
    if let Some(any_of) = sup.get("anyOf").and_then(|o| o.as_array()) {
        for option in any_of {
            if check_schema_subtype(sub, option).is_yes() {
                return SubtypeResult::Yes;
            }
        }
        return SubtypeResult::No(vec!["does not match any anyOf option".to_string()]);
    }

    // Handle allOf: sub must be subtype of all options
    if let Some(all_of) = sup.get("allOf").and_then(|o| o.as_array()) {
        let mut result = SubtypeResult::Yes;
        for option in all_of {
            result = result.and(check_schema_subtype(sub, option));
        }
        return result;
    }

    // No type and no composites - treat as any
    SubtypeResult::Yes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stepflow_core::schema::SchemaRef;

    fn schema(v: Value) -> Type {
        Type::Schema(SchemaRef::from(v))
    }

    // === Basic Type Tests ===

    #[test]
    fn test_any_is_supertype_of_everything() {
        let string = schema(json!({"type": "string"}));
        assert!(is_subtype(&string, &Type::Any).is_yes());
        assert!(is_subtype(&Type::Never, &Type::Any).is_yes());
    }

    #[test]
    fn test_any_as_subtype_is_unknown() {
        let string = schema(json!({"type": "string"}));
        assert!(is_subtype(&Type::Any, &string).is_unknown());
    }

    #[test]
    fn test_never_is_subtype_of_everything() {
        let string = schema(json!({"type": "string"}));
        assert!(is_subtype(&Type::Never, &string).is_yes());
        assert!(is_subtype(&Type::Never, &Type::Any).is_yes());
    }

    #[test]
    fn test_nothing_is_subtype_of_never() {
        let string = schema(json!({"type": "string"}));
        assert!(is_subtype(&string, &Type::Never).is_no());
    }

    #[test]
    fn test_empty_schema_is_any() {
        let string = schema(json!({"type": "string"}));
        let empty = schema(json!({}));
        assert!(is_subtype(&string, &empty).is_yes());
    }

    // === Primitive Type Tests ===

    #[test]
    fn test_same_type_is_subtype() {
        let string1 = schema(json!({"type": "string"}));
        let string2 = schema(json!({"type": "string"}));
        assert!(is_subtype(&string1, &string2).is_yes());
    }

    #[test]
    fn test_different_types_not_subtype() {
        let string = schema(json!({"type": "string"}));
        let number = schema(json!({"type": "number"}));
        let result = is_subtype(&string, &number);
        assert!(result.is_no());
        assert!(result.reasons().unwrap()[0].contains("type mismatch"));
    }

    #[test]
    fn test_enum_subtyping() {
        let subset = schema(json!({"type": "string", "enum": ["a", "b"]}));
        let superset = schema(json!({"type": "string", "enum": ["a", "b", "c"]}));

        // Subset is subtype of superset
        assert!(is_subtype(&subset, &superset).is_yes());

        // Superset is NOT subtype of subset (has extra values)
        let result = is_subtype(&superset, &subset);
        assert!(result.is_no());
    }

    #[test]
    fn test_const_subtyping() {
        let const_a = schema(json!({"type": "string", "const": "a"}));
        let const_b = schema(json!({"type": "string", "const": "b"}));
        let string = schema(json!({"type": "string"}));

        // Same const
        assert!(is_subtype(&const_a, &const_a).is_yes());

        // Different const
        assert!(is_subtype(&const_a, &const_b).is_no());

        // Const to general string is unknown (we can't verify const statically)
        // But string to const is unknown too
        assert!(is_subtype(&string, &const_a).is_unknown());
    }

    // === Object Subtyping Tests ===

    #[test]
    fn test_object_width_subtyping() {
        // More properties is subtype of fewer (width subtyping)
        let more = schema(json!({
            "type": "object",
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "number"},
                "c": {"type": "boolean"}
            }
        }));

        let fewer = schema(json!({
            "type": "object",
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "number"}
            }
        }));

        assert!(is_subtype(&more, &fewer).is_yes());
    }

    #[test]
    fn test_object_required_properties() {
        let has_required = schema(json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            },
            "required": ["name"]
        }));

        let missing_required = schema(json!({
            "type": "object",
            "properties": {
                "age": {"type": "number"}
            }
        }));

        let result = is_subtype(&missing_required, &has_required);
        assert!(result.is_no());
        assert!(result.reasons().unwrap()[0].contains("missing required property"));
    }

    #[test]
    fn test_object_depth_subtyping() {
        let nested_string = schema(json!({
            "type": "object",
            "properties": {
                "data": {"type": "string"}
            }
        }));

        let nested_number = schema(json!({
            "type": "object",
            "properties": {
                "data": {"type": "number"}
            }
        }));

        let result = is_subtype(&nested_string, &nested_number);
        assert!(result.is_no());
        assert!(result.reasons().unwrap()[0].contains("property 'data'"));
    }

    #[test]
    fn test_object_additional_properties_false() {
        let exact = schema(json!({
            "type": "object",
            "properties": {
                "a": {"type": "string"}
            },
            "additionalProperties": false
        }));

        let has_extra = schema(json!({
            "type": "object",
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "number"}
            }
        }));

        let result = is_subtype(&has_extra, &exact);
        assert!(result.is_no());
        assert!(result.reasons().unwrap()[0].contains("unexpected property"));
    }

    // === Array Subtyping Tests ===

    #[test]
    fn test_array_covariance() {
        let string_array = schema(json!({
            "type": "array",
            "items": {"type": "string"}
        }));

        let number_array = schema(json!({
            "type": "array",
            "items": {"type": "number"}
        }));

        // Same item type
        assert!(is_subtype(&string_array, &string_array).is_yes());

        // Different item type
        assert!(is_subtype(&string_array, &number_array).is_no());
    }

    // === Composite Schema Tests ===

    #[test]
    fn test_one_of_subtyping() {
        let string = schema(json!({"type": "string"}));

        let string_or_number = schema(json!({
            "oneOf": [
                {"type": "string"},
                {"type": "number"}
            ]
        }));

        // String is subtype of (string | number)
        assert!(is_subtype(&string, &string_or_number).is_yes());

        let boolean = schema(json!({"type": "boolean"}));
        // Boolean is NOT subtype of (string | number)
        assert!(is_subtype(&boolean, &string_or_number).is_no());
    }

    #[test]
    fn test_any_of_subtyping() {
        let string = schema(json!({"type": "string"}));

        let string_or_number = schema(json!({
            "anyOf": [
                {"type": "string"},
                {"type": "number"}
            ]
        }));

        assert!(is_subtype(&string, &string_or_number).is_yes());
    }

    #[test]
    fn test_all_of_subtyping() {
        let full_object = schema(json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            }
        }));

        let all_of = schema(json!({
            "allOf": [
                {
                    "type": "object",
                    "properties": {"name": {"type": "string"}}
                },
                {
                    "type": "object",
                    "properties": {"age": {"type": "number"}}
                }
            ]
        }));

        assert!(is_subtype(&full_object, &all_of).is_yes());
    }

    // === SubtypeResult Tests ===

    #[test]
    fn test_subtype_result_and() {
        assert!(matches!(
            SubtypeResult::Yes.and(SubtypeResult::Yes),
            SubtypeResult::Yes
        ));

        assert!(matches!(
            SubtypeResult::Yes.and(SubtypeResult::Unknown),
            SubtypeResult::Unknown
        ));

        let no1 = SubtypeResult::No(vec!["error1".to_string()]);
        let no2 = SubtypeResult::No(vec!["error2".to_string()]);
        if let SubtypeResult::No(reasons) = no1.and(no2) {
            assert_eq!(reasons.len(), 2);
        } else {
            panic!("Expected No");
        }
    }
}
