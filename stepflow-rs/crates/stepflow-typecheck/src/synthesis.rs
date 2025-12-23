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

//! Expression type synthesis.
//!
//! This module synthesizes types from `ValueExpr` expressions. Given a type
//! environment containing the workflow input type and step output types,
//! it computes the resulting type of any expression.
//!
//! # Key Operations
//!
//! - **Literal inference**: Infer JSON Schema from literal values
//! - **Path projection**: Navigate into schemas via JSON paths
//! - **Union types**: Combine types from branches (for `$if`, `$coalesce`)

use serde_json::Value;
use stepflow_core::schema::SchemaRef;
use stepflow_core::values::{JsonPath, PathPart, ValueExpr};

use crate::TypeEnvironment;
use crate::error::{LocatedTypeError, TypeError};
use crate::types::Type;

/// Result of type synthesis.
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// The synthesized type.
    pub ty: Type,
    /// Errors encountered during synthesis.
    pub errors: Vec<LocatedTypeError>,
}

impl SynthesisResult {
    /// Create a successful result with a type.
    pub fn ok(ty: Type) -> Self {
        SynthesisResult {
            ty,
            errors: Vec::new(),
        }
    }

    /// Create a result with an error.
    pub fn error(ty: Type, error: LocatedTypeError) -> Self {
        SynthesisResult {
            ty,
            errors: vec![error],
        }
    }

    /// Check if there were any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Synthesize the type of a value expression.
///
/// # Arguments
/// * `expr` - The expression to synthesize
/// * `env` - The type environment (input type, step types)
/// * `path` - The path to this expression (for error reporting)
///
/// # Returns
/// A `SynthesisResult` containing the synthesized type and any errors.
pub fn synthesize_type(expr: &ValueExpr, env: &TypeEnvironment, path: &str) -> SynthesisResult {
    match expr {
        ValueExpr::Step {
            step,
            path: json_path,
        } => synthesize_step_ref(step, json_path, env, path),

        ValueExpr::Input { input } => synthesize_input_ref(input, env, path),

        ValueExpr::Variable { variable, default } => {
            synthesize_variable_ref(variable, default.as_deref(), env, path)
        }

        ValueExpr::Literal(value) => SynthesisResult::ok(infer_type_from_value(value)),

        ValueExpr::EscapedLiteral { literal } => {
            SynthesisResult::ok(infer_type_from_value(literal))
        }

        ValueExpr::Array(items) => synthesize_array(items, env, path),

        ValueExpr::Object(fields) => synthesize_object(fields, env, path),

        ValueExpr::If {
            condition,
            then,
            else_expr,
        } => synthesize_if(condition, then, else_expr.as_deref(), env, path),

        ValueExpr::Coalesce { values } => synthesize_coalesce(values, env, path),
    }
}

/// Synthesize type for a step reference.
fn synthesize_step_ref(
    step_id: &str,
    json_path: &JsonPath,
    env: &TypeEnvironment,
    path: &str,
) -> SynthesisResult {
    let step_type = match env.get_step_type(step_id) {
        Some(ty) => ty.clone(),
        None => {
            return SynthesisResult::error(
                Type::Any,
                LocatedTypeError::at_flow(TypeError::unknown_step(step_id), path),
            );
        }
    };

    // Project through the path
    project_type(&step_type, json_path, path)
}

/// Synthesize type for an input reference.
fn synthesize_input_ref(
    json_path: &JsonPath,
    env: &TypeEnvironment,
    path: &str,
) -> SynthesisResult {
    let input_type = env.input_type().clone();
    project_type(&input_type, json_path, path)
}

/// Synthesize type for a variable reference.
fn synthesize_variable_ref(
    variable_path: &JsonPath,
    default: Option<&ValueExpr>,
    env: &TypeEnvironment,
    path: &str,
) -> SynthesisResult {
    // Extract variable name from the first part of the path
    let parts = variable_path.parts();
    let var_name = match parts.first() {
        Some(PathPart::Field(name)) | Some(PathPart::IndexStr(name)) => name.as_str(),
        _ => {
            return SynthesisResult::error(
                Type::Any,
                LocatedTypeError::at_flow(TypeError::indeterminate("invalid variable path"), path),
            );
        }
    };

    let base_type = match env.get_variable_type(var_name) {
        Some(ty) => ty.clone(),
        None => {
            // If there's a default, use its type
            if let Some(default_expr) = default {
                return synthesize_type(default_expr, env, path);
            }
            return SynthesisResult::error(
                Type::Any,
                LocatedTypeError::at_flow(TypeError::unknown_variable(var_name), path),
            );
        }
    };

    // Apply remaining path if any
    if parts.len() > 1 {
        let remaining_path = JsonPath::from_parts(parts[1..].to_vec());
        let mut result = project_type(&base_type, &remaining_path, path);

        // If projection fails and there's a default, union with default type
        if let Some(default_expr) = default.filter(|_| result.has_errors()) {
            let default_result = synthesize_type(default_expr, env, path);
            result.ty = union_types(&result.ty, &default_result.ty);
            result.errors.extend(default_result.errors);
        }

        result
    } else if let Some(default_expr) = default {
        // Variable found but might be null, so union with default type
        let default_result = synthesize_type(default_expr, env, path);
        let union_ty = union_types(&base_type, &default_result.ty);
        SynthesisResult {
            ty: union_ty,
            errors: default_result.errors,
        }
    } else {
        SynthesisResult::ok(base_type)
    }
}

/// Synthesize type for an array expression.
fn synthesize_array(items: &[ValueExpr], env: &TypeEnvironment, path: &str) -> SynthesisResult {
    let mut all_errors = Vec::new();
    let mut item_types = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let item_path = format!("{}[{}]", path, i);
        let result = synthesize_type(item, env, &item_path);
        all_errors.extend(result.errors);
        item_types.push(result.ty);
    }

    // Build array schema from item types
    let items_schema = if item_types.is_empty() {
        serde_json::json!({})
    } else {
        // Compute union of all item types
        let unified = item_types
            .iter()
            .fold(Type::Never, |acc, ty| union_types(&acc, ty));

        match unified {
            Type::Schema(s) => s.as_value().clone(),
            Type::Any => serde_json::json!({}),
            Type::Never => serde_json::json!({}),
        }
    };

    let schema = serde_json::json!({
        "type": "array",
        "items": items_schema
    });

    SynthesisResult {
        ty: Type::Schema(SchemaRef::from(schema)),
        errors: all_errors,
    }
}

/// Synthesize type for an object expression.
fn synthesize_object(
    fields: &[(String, ValueExpr)],
    env: &TypeEnvironment,
    path: &str,
) -> SynthesisResult {
    let mut all_errors = Vec::new();
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (name, value) in fields {
        let field_path = format!("{}.{}", path, name);
        let result = synthesize_type(value, env, &field_path);
        all_errors.extend(result.errors);

        let prop_schema = match result.ty {
            Type::Schema(s) => s.as_value().clone(),
            Type::Any => serde_json::json!({}),
            Type::Never => serde_json::json!({}),
        };

        properties.insert(name.clone(), prop_schema);
        required.push(Value::String(name.clone()));
    }

    let schema = serde_json::json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": required
    });

    SynthesisResult {
        ty: Type::Schema(SchemaRef::from(schema)),
        errors: all_errors,
    }
}

/// Synthesize type for an if expression.
fn synthesize_if(
    condition: &ValueExpr,
    then_branch: &ValueExpr,
    else_branch: Option<&ValueExpr>,
    env: &TypeEnvironment,
    path: &str,
) -> SynthesisResult {
    let mut all_errors = Vec::new();

    // Synthesize condition (we don't check its type, just collect errors)
    let cond_result = synthesize_type(condition, env, &format!("{}.$if", path));
    all_errors.extend(cond_result.errors);

    // Synthesize then branch
    let then_result = synthesize_type(then_branch, env, &format!("{}.then", path));
    all_errors.extend(then_result.errors);

    // Synthesize else branch or use null
    let else_type = if let Some(else_expr) = else_branch {
        let else_result = synthesize_type(else_expr, env, &format!("{}.else", path));
        all_errors.extend(else_result.errors);
        else_result.ty
    } else {
        // No else branch returns null
        Type::Schema(SchemaRef::from(serde_json::json!({"type": "null"})))
    };

    // Result is union of both branches
    let result_type = union_types(&then_result.ty, &else_type);

    SynthesisResult {
        ty: result_type,
        errors: all_errors,
    }
}

/// Synthesize type for a coalesce expression.
fn synthesize_coalesce(values: &[ValueExpr], env: &TypeEnvironment, path: &str) -> SynthesisResult {
    let mut all_errors = Vec::new();
    let mut types = Vec::new();

    for (i, value) in values.iter().enumerate() {
        let val_path = format!("{}.$coalesce[{}]", path, i);
        let result = synthesize_type(value, env, &val_path);
        all_errors.extend(result.errors);
        types.push(result.ty);
    }

    // Result type is union of all value types
    let result_type = types
        .iter()
        .fold(Type::Never, |acc, ty| union_types(&acc, ty));

    SynthesisResult {
        ty: result_type,
        errors: all_errors,
    }
}

/// Project a type through a JSON path.
fn project_type(base: &Type, path: &JsonPath, loc: &str) -> SynthesisResult {
    if path.is_empty() {
        return SynthesisResult::ok(base.clone());
    }

    match base {
        Type::Any => SynthesisResult::ok(Type::Any),
        Type::Never => SynthesisResult::ok(Type::Never),
        Type::Schema(schema) => project_schema_path(schema.as_value(), path, loc),
    }
}

/// Project a JSON schema through a path.
fn project_schema_path(schema: &Value, path: &JsonPath, loc: &str) -> SynthesisResult {
    let mut current = schema.clone();

    for part in path.parts() {
        match part {
            PathPart::Field(name) | PathPart::IndexStr(name) => {
                // Navigate into object properties
                if let Some(prop_schema) =
                    current.get("properties").and_then(|p| p.get(name.as_str()))
                {
                    current = prop_schema.clone();
                    continue;
                }
                // Check additionalProperties
                if let Some(additional) = current.get("additionalProperties") {
                    if additional.is_boolean() && additional.as_bool() == Some(true) {
                        // additionalProperties: true means any type
                        return SynthesisResult::ok(Type::Any);
                    } else if additional.is_object() {
                        current = additional.clone();
                        continue;
                    }
                }
                // Property not found
                return SynthesisResult::error(
                    Type::Any,
                    LocatedTypeError::at_flow(
                        TypeError::property_not_found(path.to_string(), name.clone()),
                        loc,
                    ),
                );
            }
            PathPart::Index(_) => {
                // Navigate into array items
                if let Some(items) = current.get("items") {
                    current = items.clone();
                    continue;
                }
                // No items schema - return Any
                return SynthesisResult::ok(Type::Any);
            }
        }
    }

    SynthesisResult::ok(Type::Schema(SchemaRef::from(current)))
}

/// Infer a type from a JSON value.
pub fn infer_type_from_value(value: &Value) -> Type {
    let schema = match value {
        Value::Null => serde_json::json!({"type": "null"}),
        Value::Bool(_) => serde_json::json!({"type": "boolean"}),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                serde_json::json!({"type": "integer"})
            } else {
                serde_json::json!({"type": "number"})
            }
        }
        Value::String(_) => serde_json::json!({"type": "string"}),
        Value::Array(items) => {
            if items.is_empty() {
                serde_json::json!({"type": "array"})
            } else {
                // Union of all item types
                let item_types: Vec<_> = items.iter().map(infer_type_from_value).collect();
                let unified = item_types
                    .iter()
                    .fold(Type::Never, |acc, ty| union_types(&acc, ty));

                let items_schema = match unified {
                    Type::Schema(s) => s.as_value().clone(),
                    _ => serde_json::json!({}),
                };

                serde_json::json!({
                    "type": "array",
                    "items": items_schema
                })
            }
        }
        Value::Object(map) => {
            let mut properties = serde_json::Map::new();
            let required: Vec<Value> = map.keys().map(|k| Value::String(k.clone())).collect();

            for (k, v) in map {
                let prop_type = infer_type_from_value(v);
                let prop_schema = match prop_type {
                    Type::Schema(s) => s.as_value().clone(),
                    _ => serde_json::json!({}),
                };
                properties.insert(k.clone(), prop_schema);
            }

            serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required
            })
        }
    };

    Type::Schema(SchemaRef::from(schema))
}

/// Compute the union of two types.
///
/// The result is a type that accepts values from either input type.
pub fn union_types(a: &Type, b: &Type) -> Type {
    match (a, b) {
        // Any unions with anything is Any
        (Type::Any, _) | (_, Type::Any) => Type::Any,

        // Never is identity for union
        (Type::Never, t) | (t, Type::Never) => t.clone(),

        // Two schemas - check if they're equal or build anyOf
        (Type::Schema(s1), Type::Schema(s2)) => {
            if s1 == s2 {
                Type::Schema(s1.clone())
            } else {
                // Try to simplify common cases
                let v1 = s1.as_value();
                let v2 = s2.as_value();

                // If both have same type, might be able to merge
                let t1 = v1.get("type").and_then(|t| t.as_str());
                let t2 = v2.get("type").and_then(|t| t.as_str());

                if t1 == t2 {
                    // Same base type - for now just return the first one
                    // (A more sophisticated implementation would merge properties)
                    Type::Schema(s1.clone())
                } else {
                    // Different types - build anyOf
                    let schema = serde_json::json!({
                        "anyOf": [v1, v2]
                    });
                    Type::Schema(SchemaRef::from(schema))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_env() -> TypeEnvironment {
        let mut env = TypeEnvironment::new();

        // Set up input type
        env.set_input_type(Type::from_json(json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "count": {"type": "integer"},
                "nested": {
                    "type": "object",
                    "properties": {
                        "value": {"type": "number"}
                    }
                }
            },
            "required": ["name", "count"]
        })));

        // Set up step types
        env.set_step_type(
            "step1".to_string(),
            Type::from_json(json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"},
                    "data": {
                        "type": "object",
                        "properties": {
                            "items": {
                                "type": "array",
                                "items": {"type": "number"}
                            }
                        }
                    }
                }
            })),
        );

        env.set_step_type(
            "step2".to_string(),
            Type::from_json(json!({"type": "number"})),
        );

        // Set up variable types
        env.set_variable_type(
            "api_key".to_string(),
            Type::from_json(json!({"type": "string"})),
        );

        env
    }

    // === Literal Tests ===

    #[test]
    fn test_synthesize_literal_null() {
        let env = make_env();
        let expr = ValueExpr::literal(json!(null));
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "null");
    }

    #[test]
    fn test_synthesize_literal_string() {
        let env = make_env();
        let expr = ValueExpr::literal(json!("hello"));
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "string");
    }

    #[test]
    fn test_synthesize_literal_number() {
        let env = make_env();
        let expr = ValueExpr::literal(json!(42));
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "integer");
    }

    #[test]
    fn test_synthesize_literal_object() {
        let env = make_env();
        let expr = ValueExpr::literal(json!({"a": 1, "b": "hello"}));
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        let schema = result.ty.as_schema().unwrap();
        assert_eq!(schema.as_value().get("type").unwrap(), "object");
    }

    // === Input Reference Tests ===

    #[test]
    fn test_synthesize_input_root() {
        let env = make_env();
        let expr = ValueExpr::workflow_input(JsonPath::new());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Now shows object properties with types
        let display = result.ty.to_string();
        assert!(display.contains("name: string"), "display: {}", display);
        assert!(display.contains("count: integer"), "display: {}", display);
    }

    #[test]
    fn test_synthesize_input_field() {
        let env = make_env();
        let expr = ValueExpr::workflow_input(JsonPath::parse("$.name").unwrap());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "string");
    }

    #[test]
    fn test_synthesize_input_nested() {
        let env = make_env();
        let expr = ValueExpr::workflow_input(JsonPath::parse("$.nested.value").unwrap());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "number");
    }

    #[test]
    fn test_synthesize_input_missing_field() {
        let env = make_env();
        let expr = ValueExpr::workflow_input(JsonPath::parse("$.nonexistent").unwrap());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(result.has_errors());
        assert!(result.ty.is_any()); // Falls back to Any
    }

    // === Step Reference Tests ===

    #[test]
    fn test_synthesize_step_ref() {
        let env = make_env();
        let expr = ValueExpr::step("step1", JsonPath::new());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Now shows object properties with types
        let display = result.ty.to_string();
        assert!(display.contains("result: string"), "display: {}", display);
        assert!(display.contains("data:"), "display: {}", display);
    }

    #[test]
    fn test_synthesize_step_ref_with_path() {
        let env = make_env();
        let expr = ValueExpr::step("step1", JsonPath::parse("$.result").unwrap());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "string");
    }

    #[test]
    fn test_synthesize_step_ref_nested_path() {
        let env = make_env();
        let expr = ValueExpr::step("step1", JsonPath::parse("$.data.items").unwrap());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Now shows array item type
        assert_eq!(result.ty.to_string(), "number[]");
    }

    #[test]
    fn test_synthesize_unknown_step() {
        let env = make_env();
        let expr = ValueExpr::step("unknown_step", JsonPath::new());
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(result.has_errors());
        assert!(matches!(
            &result.errors[0].error,
            TypeError::UnknownStep { step_id } if step_id == "unknown_step"
        ));
    }

    // === Variable Reference Tests ===

    #[test]
    fn test_synthesize_variable() {
        let env = make_env();
        let expr = ValueExpr::variable("api_key", None);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "string");
    }

    #[test]
    fn test_synthesize_unknown_variable() {
        let env = make_env();
        let expr = ValueExpr::variable("unknown_var", None);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(result.has_errors());
        assert!(matches!(
            &result.errors[0].error,
            TypeError::UnknownVariable { variable } if variable == "unknown_var"
        ));
    }

    #[test]
    fn test_synthesize_variable_with_default() {
        let env = make_env();
        let default = Box::new(ValueExpr::literal(json!("default_value")));
        let expr = ValueExpr::variable("unknown_var", Some(default));
        let result = synthesize_type(&expr, &env, "$.test");

        // Should use default type when variable not found
        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "string");
    }

    // === Array Expression Tests ===

    #[test]
    fn test_synthesize_array_empty() {
        let env = make_env();
        let expr = ValueExpr::array(vec![]);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Empty array shows item type as any
        assert_eq!(result.ty.to_string(), "any[]");
    }

    #[test]
    fn test_synthesize_array_homogeneous() {
        let env = make_env();
        let expr = ValueExpr::array(vec![
            ValueExpr::literal(json!(1)),
            ValueExpr::literal(json!(2)),
            ValueExpr::literal(json!(3)),
        ]);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        let schema = result.ty.as_schema().unwrap();
        let items = schema.as_value().get("items").unwrap();
        assert_eq!(items.get("type").unwrap(), "integer");
    }

    #[test]
    fn test_synthesize_array_with_references() {
        let env = make_env();
        let expr = ValueExpr::array(vec![
            ValueExpr::step("step2", JsonPath::new()),
            ValueExpr::literal(json!(42)),
        ]);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // step2 returns number, literal is integer -> union shows as union type
        let display = result.ty.to_string();
        // The display should show array with item type info
        assert!(
            display.contains("[]") || display == "array",
            "display: {}",
            display
        );
    }

    // === Object Expression Tests ===

    #[test]
    fn test_synthesize_object() {
        let env = make_env();
        let expr = ValueExpr::object(vec![
            ("name".to_string(), ValueExpr::literal(json!("test"))),
            (
                "value".to_string(),
                ValueExpr::step("step2", JsonPath::new()),
            ),
        ]);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        let schema = result.ty.as_schema().unwrap();
        assert_eq!(schema.as_value().get("type").unwrap(), "object");

        let props = schema.as_value().get("properties").unwrap();
        assert_eq!(props.get("name").unwrap().get("type").unwrap(), "string");
        assert_eq!(props.get("value").unwrap().get("type").unwrap(), "number");
    }

    // === If Expression Tests ===

    #[test]
    fn test_synthesize_if_same_types() {
        let env = make_env();
        let expr = ValueExpr::if_expr(
            ValueExpr::literal(json!(true)),
            ValueExpr::literal(json!("then")),
            Some(ValueExpr::literal(json!("else"))),
        );
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        assert_eq!(result.ty.to_string(), "string");
    }

    #[test]
    fn test_synthesize_if_no_else() {
        let env = make_env();
        let expr = ValueExpr::if_expr(
            ValueExpr::literal(json!(true)),
            ValueExpr::literal(json!("then")),
            None,
        );
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Should be string | null (anyOf)
        let schema = result.ty.as_schema().unwrap();
        assert!(schema.as_value().get("anyOf").is_some());
    }

    #[test]
    fn test_synthesize_if_different_types() {
        let env = make_env();
        let expr = ValueExpr::if_expr(
            ValueExpr::literal(json!(true)),
            ValueExpr::literal(json!("string_value")),
            Some(ValueExpr::literal(json!(42))),
        );
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Should be string | integer (anyOf)
        let schema = result.ty.as_schema().unwrap();
        assert!(schema.as_value().get("anyOf").is_some());
    }

    // === Coalesce Expression Tests ===

    #[test]
    fn test_synthesize_coalesce() {
        let env = make_env();
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::step("step1", JsonPath::parse("$.result").unwrap()),
            ValueExpr::literal(json!("fallback")),
        ]);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        // Both are strings, so result is string
        assert_eq!(result.ty.to_string(), "string");
    }

    // === Union Type Tests ===

    #[test]
    fn test_union_with_never() {
        let string = Type::from_json(json!({"type": "string"}));
        let result = union_types(&Type::Never, &string);
        assert_eq!(result, string);
    }

    #[test]
    fn test_union_with_any() {
        let string = Type::from_json(json!({"type": "string"}));
        let result = union_types(&Type::Any, &string);
        assert!(result.is_any());
    }

    #[test]
    fn test_union_same_types() {
        let string1 = Type::from_json(json!({"type": "string"}));
        let string2 = Type::from_json(json!({"type": "string"}));
        let result = union_types(&string1, &string2);
        assert_eq!(result.to_string(), "string");
    }

    // === Infer Type From Value Tests ===

    #[test]
    fn test_infer_type_primitives() {
        assert_eq!(infer_type_from_value(&json!(null)).to_string(), "null");
        assert_eq!(infer_type_from_value(&json!(true)).to_string(), "boolean");
        assert_eq!(infer_type_from_value(&json!(42)).to_string(), "integer");
        assert_eq!(infer_type_from_value(&json!(3.14)).to_string(), "number");
        assert_eq!(infer_type_from_value(&json!("hello")).to_string(), "string");
    }

    #[test]
    fn test_infer_type_array() {
        let ty = infer_type_from_value(&json!([1, 2, 3]));
        // Now shows item type
        assert_eq!(ty.to_string(), "integer[]");
    }

    #[test]
    fn test_infer_type_object() {
        let ty = infer_type_from_value(&json!({"a": 1}));
        // Now shows property types
        assert_eq!(ty.to_string(), "{ a: integer }");
    }

    // === Secret Propagation Tests ===

    fn make_env_with_secrets() -> TypeEnvironment {
        let mut env = TypeEnvironment::new();

        // Set up input type
        env.set_input_type(Type::from_json(json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        })));

        // Set up a secret variable (api_key marked with is_secret: true)
        env.set_variable_type(
            "api_key".to_string(),
            Type::from_json(json!({
                "type": "string",
                "is_secret": true
            })),
        );

        // Set up a non-secret variable
        env.set_variable_type(
            "temperature".to_string(),
            Type::from_json(json!({
                "type": "number"
            })),
        );

        env
    }

    #[test]
    fn test_synthesize_secret_variable_preserves_is_secret() {
        let env = make_env_with_secrets();
        let expr = ValueExpr::variable("api_key", None);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        let schema = result.ty.as_schema().unwrap();
        // The is_secret annotation should be preserved
        assert_eq!(
            schema.as_value().get("is_secret").and_then(|v| v.as_bool()),
            Some(true),
            "is_secret should be preserved in variable reference"
        );
    }

    #[test]
    fn test_synthesize_non_secret_variable_has_no_is_secret() {
        let env = make_env_with_secrets();
        let expr = ValueExpr::variable("temperature", None);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        let schema = result.ty.as_schema().unwrap();
        // Non-secret variables should not have is_secret
        assert!(
            schema.as_value().get("is_secret").is_none(),
            "non-secret variable should not have is_secret"
        );
    }

    #[test]
    fn test_synthesize_object_with_secret_variable_propagates_is_secret() {
        let env = make_env_with_secrets();
        // Synthesize: { api_key: {$variable: "api_key"}, temp: {$variable: "temperature"} }
        let expr = ValueExpr::object(vec![
            ("api_key".to_string(), ValueExpr::variable("api_key", None)),
            ("temp".to_string(), ValueExpr::variable("temperature", None)),
        ]);
        let result = synthesize_type(&expr, &env, "$.test");

        assert!(!result.has_errors());
        let schema = result.ty.as_schema().unwrap();
        let props = schema.as_value().get("properties").unwrap();

        // api_key property should have is_secret: true
        let api_key_prop = props.get("api_key").unwrap();
        assert_eq!(
            api_key_prop.get("is_secret").and_then(|v| v.as_bool()),
            Some(true),
            "api_key property should have is_secret: true"
        );

        // temp property should not have is_secret
        let temp_prop = props.get("temp").unwrap();
        assert!(
            temp_prop.get("is_secret").is_none(),
            "temp property should not have is_secret"
        );
    }
}
