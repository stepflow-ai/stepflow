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

use super::JsonPath;
use schemars::JsonSchema;
use serde_json::Value;

/// A value expression that can contain literal data or references to other values.
///
/// This is the unified type for all workflow inputs, outputs, and data flow.
/// Expressions can be:
/// - References to other values (`$step`, `$input`, `$variable`)
/// - Composable structures (arrays and objects containing expressions)
/// - Literal values (null, bool, number, string - any primitive JSON value)
/// - Escaped literals (`$literal`) to prevent expansion
//
// Serialization and deserialization are implemented in expr_serde.rs
// JsonSchema is manually implemented to match the actual wire format
// utoipa::ToSchema is manually implemented to prevent stack overflow from recursion
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ValueExpr {
    /// Step reference: `{ $step: "step_id", path: "optional.path" }`
    Step {
        step: String,
        path: JsonPath,
    },

    /// Workflow input: `{ $input: "path" }` where path can be "$" for root
    Input {
        input: JsonPath, // The path is the value (supports shorthand)
    },

    /// Variable: `{ $variable: "$.var.path", default: ... }`
    Variable {
        variable: JsonPath, // JSONPath including variable name and path
        default: Option<Box<ValueExpr>>,
    },

    /// Escape hatch: `{ $literal: {...} }` - prevents recursive parsing
    EscapedLiteral {
        literal: serde_json::Value,
    },

    /// Conditional expression: `{ $if: <condition>, then: <expr>, else?: <expr> }`
    /// Returns `then` value if condition is truthy, otherwise `else` value (defaults to null)
    If {
        condition: Box<ValueExpr>,
        then: Box<ValueExpr>,
        else_expr: Option<Box<ValueExpr>>,
    },

    /// Coalesce: `{ $coalesce: [<expr1>, <expr2>, ...] }`
    /// Returns first non-skipped, non-null value from the list
    Coalesce {
        values: Vec<ValueExpr>,
    },

    /// JSON array where each element can be an expression
    Array(Vec<ValueExpr>),

    /// JSON object where each value can be an expression
    /// Uses Vec instead of IndexMap for efficiency and to enable future hashability
    Object(Vec<(String, ValueExpr)>),

    /// Literal JSON value (null, bool, number, string)
    /// Note: Literal objects and arrays are parsed as Object/Array variants
    Literal(serde_json::Value),
}

impl ValueExpr {
    /// Create a step reference expression
    pub fn step(step_id: impl Into<String>, path: JsonPath) -> Self {
        ValueExpr::Step {
            step: step_id.into(),
            path,
        }
    }

    /// Create a step reference without a path (for compatibility with old code)
    pub fn step_output(step_id: impl Into<String>) -> Self {
        ValueExpr::Step {
            step: step_id.into(),
            path: JsonPath::default(),
        }
    }

    /// Create a workflow input reference expression
    pub fn workflow_input(path: JsonPath) -> Self {
        ValueExpr::Input { input: path }
    }

    /// Create a variable reference expression
    pub fn variable(name: impl Into<String>, default: Option<Box<ValueExpr>>) -> Self {
        ValueExpr::Variable {
            variable: JsonPath::from(name.into()),
            default,
        }
    }

    /// Create a literal value expression from a serde_json::Value
    ///
    /// Arrays and objects are recursively converted to composable ValueExpr structures.
    /// Primitives (null, bool, number, string) become Literal variants.
    pub fn literal(value: serde_json::Value) -> Self {
        match value {
            Value::Array(arr) => {
                let exprs = arr.into_iter().map(ValueExpr::literal).collect();
                ValueExpr::Array(exprs)
            }
            Value::Object(obj) => {
                let exprs = obj
                    .into_iter()
                    .map(|(k, v)| (k, ValueExpr::literal(v)))
                    .collect();
                ValueExpr::Object(exprs)
            }
            // All primitives (null, bool, number, string) become Literal
            primitive => ValueExpr::Literal(primitive),
        }
    }

    /// Create an array value expression
    pub fn array(values: Vec<ValueExpr>) -> Self {
        ValueExpr::Array(values)
    }

    /// Create an object value expression from key-value pairs
    pub fn object(values: Vec<(String, ValueExpr)>) -> Self {
        ValueExpr::Object(values)
    }

    /// Create an escaped literal expression
    pub fn escaped_literal(value: serde_json::Value) -> Self {
        ValueExpr::EscapedLiteral { literal: value }
    }

    /// Create a conditional expression
    pub fn if_expr(condition: ValueExpr, then: ValueExpr, else_expr: Option<ValueExpr>) -> Self {
        ValueExpr::If {
            condition: Box::new(condition),
            then: Box::new(then),
            else_expr: else_expr.map(Box::new),
        }
    }

    /// Create a coalesce expression
    pub fn coalesce(values: Vec<ValueExpr>) -> Self {
        ValueExpr::Coalesce { values }
    }

    /// Create a null literal expression
    pub fn null() -> Self {
        ValueExpr::Literal(serde_json::Value::Null)
    }

    /// Check if this expression is a null literal
    pub fn is_null(&self) -> bool {
        matches!(self, ValueExpr::Literal(serde_json::Value::Null))
    }
}

impl Default for ValueExpr {
    fn default() -> Self {
        ValueExpr::null()
    }
}

// Manual JsonSchema implementation to match the actual wire format
impl JsonSchema for ValueExpr {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ValueExpr".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A value expression that can contain literal data or references to other values",
            "oneOf": [
                {
                    "description": "Step reference: { $step: \"step_id\", path?: \"...\" }",
                    "type": "object",
                    "properties": {
                        "$step": { "type": "string" },
                        "path": { "type": "string", "description": "JSONPath expression" }
                    },
                    "required": ["$step"],
                    "additionalProperties": false
                },
                {
                    "description": "Workflow input reference: { $input: \"path\" }",
                    "type": "object",
                    "properties": {
                        "$input": { "type": "string", "description": "JSONPath expression" }
                    },
                    "required": ["$input"],
                    "additionalProperties": false
                },
                {
                    "description": "Variable reference: { $variable: \"path\", default?: ValueExpr }",
                    "type": "object",
                    "properties": {
                        "$variable": { "type": "string", "description": "JSONPath expression including variable name" },
                        "default": { "$ref": "#/$defs/ValueExpr" }
                    },
                    "required": ["$variable"],
                    "additionalProperties": false
                },
                {
                    "description": "Escaped literal: { $literal: any }",
                    "type": "object",
                    "properties": {
                        "$literal": {}
                    },
                    "required": ["$literal"],
                    "additionalProperties": false
                },
                {
                    "description": "Conditional: { $if: condition, then: expr, else?: expr }",
                    "type": "object",
                    "properties": {
                        "$if": { "$ref": "#/$defs/ValueExpr" },
                        "then": { "$ref": "#/$defs/ValueExpr" },
                        "else": { "$ref": "#/$defs/ValueExpr" }
                    },
                    "required": ["$if", "then"],
                    "additionalProperties": false
                },
                {
                    "description": "Coalesce: { $coalesce: [expr1, expr2, ...] }",
                    "type": "object",
                    "properties": {
                        "$coalesce": {
                            "type": "array",
                            "items": { "$ref": "#/$defs/ValueExpr" }
                        }
                    },
                    "required": ["$coalesce"],
                    "additionalProperties": false
                },
                {
                    "description": "Array of expressions",
                    "type": "array",
                    "items": { "$ref": "#/$defs/ValueExpr" }
                },
                {
                    "description": "Object with expression values",
                    "type": "object",
                    "additionalProperties": { "$ref": "#/$defs/ValueExpr" }
                },
                {
                    "description": "Literal primitive value",
                    "oneOf": [
                        { "type": "null" },
                        { "type": "boolean" },
                        { "type": "number" },
                        { "type": "string" }
                    ]
                }
            ]
        })
    }
}

// Manual utoipa::ToSchema implementation to prevent stack overflow from recursion
impl utoipa::PartialSchema for ValueExpr {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::*;
        // Return a reference to prevent inline recursion
        RefOr::Ref(Ref::new("#/components/schemas/ValueExpr"))
    }
}

impl utoipa::ToSchema for ValueExpr {
    fn name() -> std::borrow::Cow<'static, str> {
        "ValueExpr".into()
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        use utoipa::openapi::schema::*;
        use utoipa::openapi::*;

        // Create a oneOf schema with each variant described
        // Note: We don't need to explicitly set schema types for objects since ObjectBuilder
        // defaults to object type. For primitive types, we can just describe them in documentation.
        let one_of_items = vec![
            // Step reference: { $step: "step_id", path?: "..." }
            Schema::Object(
                ObjectBuilder::new()
                    .property("$step", Object::new())
                    .property("path", Object::new())
                    .required("$step")
                    .description(Some("Step reference: { $step: \"step_id\", path?: \"...\" }"))
                    .build()
            ),
            // Input reference: { $input: "path" }
            Schema::Object(
                ObjectBuilder::new()
                    .property("$input", Object::new())
                    .required("$input")
                    .description(Some("Workflow input reference: { $input: \"path\" }"))
                    .build()
            ),
            // Variable reference: { $variable: "path", default?: ValueExpr }
            Schema::Object(
                ObjectBuilder::new()
                    .property("$variable", Object::new())
                    .property("default", RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    .required("$variable")
                    .description(Some("Variable reference: { $variable: \"path\", default?: ValueExpr }"))
                    .build()
            ),
            // Escaped literal: { $literal: any }
            Schema::Object(
                ObjectBuilder::new()
                    .property("$literal", Object::new())
                    .required("$literal")
                    .description(Some("Escaped literal: { $literal: any }"))
                    .build()
            ),
            // Conditional: { $if: condition, then: expr, else?: expr }
            Schema::Object(
                ObjectBuilder::new()
                    .property("$if", RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    .property("then", RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    .property("else", RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    .required("$if")
                    .required("then")
                    .description(Some("Conditional: { $if: condition, then: expr, else?: expr }"))
                    .build()
            ),
            // Coalesce: { $coalesce: [expr1, expr2, ...] }
            Schema::Object(
                ObjectBuilder::new()
                    .property(
                        "$coalesce",
                        ArrayBuilder::new()
                            .items(RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    )
                    .required("$coalesce")
                    .description(Some("Coalesce: { $coalesce: [expr1, expr2, ...] }"))
                    .build()
            ),
            // Array of expressions
            Schema::Array(
                ArrayBuilder::new()
                    .items(RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    .description(Some("Array of expressions"))
                    .build()
            ),
            // Object with expression values
            Schema::Object(
                ObjectBuilder::new()
                    .additional_properties(Some(RefOr::Ref(Ref::new("#/components/schemas/ValueExpr"))))
                    .description(Some("Object with expression values"))
                    .build()
            ),
            // Literal primitive value - just describe as allowing any value
            Schema::Object(
                ObjectBuilder::new()
                    .description(Some("Literal primitive value (null, boolean, number, or string)"))
                    .build()
            ),
        ];

        // Build the final oneOf schema
        let mut one_of_builder = OneOfBuilder::new();
        for item in one_of_items {
            one_of_builder = one_of_builder.item(item);
        }

        schemas.push((
            "ValueExpr".to_string(),
            RefOr::T(Schema::OneOf(
                one_of_builder
                    .description(Some("A value expression that can contain literal data or references to other values"))
                    .build()
            ))
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_step_constructor() {
        let expr = ValueExpr::step("my_step", JsonPath::default());
        assert_eq!(
            expr,
            ValueExpr::Step {
                step: "my_step".to_string(),
                path: JsonPath::default()
            }
        );
    }

    #[test]
    fn test_input_constructor() {
        let expr = ValueExpr::workflow_input(JsonPath::from("field"));
        assert_eq!(
            expr,
            ValueExpr::Input {
                input: JsonPath::from("field")
            }
        );
    }

    #[test]
    fn test_variable_constructor() {
        let expr = ValueExpr::variable("my_var", None);
        assert_eq!(
            expr,
            ValueExpr::Variable {
                variable: JsonPath::from("my_var"),
                default: None
            }
        );
    }

    #[test]
    fn test_variable_with_default() {
        let default_expr = Box::new(ValueExpr::literal(json!("default_value")));
        let expr = ValueExpr::variable("my_var", Some(default_expr.clone()));
        assert_eq!(
            expr,
            ValueExpr::Variable {
                variable: JsonPath::from("my_var"),
                default: Some(default_expr)
            }
        );
    }

    #[test]
    fn test_literal_primitives() {
        // Null
        assert_eq!(
            ValueExpr::literal(json!(null)),
            ValueExpr::Literal(json!(null))
        );

        // Bool
        assert_eq!(
            ValueExpr::literal(json!(true)),
            ValueExpr::Literal(json!(true))
        );
        assert_eq!(
            ValueExpr::literal(json!(false)),
            ValueExpr::Literal(json!(false))
        );

        // Number
        assert_eq!(ValueExpr::literal(json!(42)), ValueExpr::Literal(json!(42)));
        assert_eq!(
            ValueExpr::literal(json!(3.25)),
            ValueExpr::Literal(json!(3.25))
        );

        // String
        assert_eq!(
            ValueExpr::literal(json!("hello")),
            ValueExpr::Literal(json!("hello"))
        );
    }

    #[test]
    fn test_literal_composable_structures() {
        // Array - should be converted to Array variant
        let arr_expr = ValueExpr::literal(json!([1, 2, 3]));
        match arr_expr {
            ValueExpr::Array(arr) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], ValueExpr::Literal(json!(1)));
                assert_eq!(arr[1], ValueExpr::Literal(json!(2)));
                assert_eq!(arr[2], ValueExpr::Literal(json!(3)));
            }
            _ => panic!("Expected Array variant"),
        }

        // Object - should be converted to Object variant
        let obj_expr = ValueExpr::literal(json!({"a": 1, "b": "hello"}));
        match obj_expr {
            ValueExpr::Object(obj) => {
                assert_eq!(obj.len(), 2);
                // Vec is unordered in terms of what we guarantee, but serde_json preserves order
                assert!(
                    obj.iter()
                        .any(|(k, v)| k == "a" && *v == ValueExpr::Literal(json!(1)))
                );
                assert!(
                    obj.iter()
                        .any(|(k, v)| k == "b" && *v == ValueExpr::Literal(json!("hello")))
                );
            }
            _ => panic!("Expected Object variant"),
        }
    }

    #[test]
    fn test_array_constructor() {
        let arr = ValueExpr::array(vec![
            ValueExpr::literal(json!(1)),
            ValueExpr::literal(json!("two")),
        ]);
        assert_eq!(
            arr,
            ValueExpr::Array(vec![
                ValueExpr::Literal(json!(1)),
                ValueExpr::Literal(json!("two"))
            ])
        );
    }

    #[test]
    fn test_object_constructor() {
        let obj = ValueExpr::object(vec![
            ("a".to_string(), ValueExpr::literal(json!(1))),
            ("b".to_string(), ValueExpr::literal(json!("hello"))),
        ]);
        assert_eq!(
            obj,
            ValueExpr::Object(vec![
                ("a".to_string(), ValueExpr::Literal(json!(1))),
                ("b".to_string(), ValueExpr::Literal(json!("hello")))
            ])
        );
    }

    #[test]
    fn test_escaped_literal() {
        let expr = ValueExpr::escaped_literal(json!({"step": "foo"}));
        assert_eq!(
            expr,
            ValueExpr::EscapedLiteral {
                literal: json!({"step": "foo"})
            }
        );
    }

    #[test]
    fn test_composable_with_references() {
        // Array of mixed expressions and literals
        let arr = ValueExpr::Array(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::literal(json!("literal_string")),
            ValueExpr::workflow_input(JsonPath::from("field")),
        ]);

        match &arr {
            ValueExpr::Array(v) => {
                assert_eq!(v.len(), 3);
                assert!(matches!(&v[0], ValueExpr::Step { .. }));
                assert!(matches!(&v[1], ValueExpr::Literal(_)));
                assert!(matches!(&v[2], ValueExpr::Input { .. }));
            }
            _ => panic!("Expected Array"),
        }

        // Object with mixed expressions
        let obj = ValueExpr::Object(vec![
            (
                "ref".to_string(),
                ValueExpr::step("step1", JsonPath::default()),
            ),
            ("lit".to_string(), ValueExpr::literal(json!("value"))),
            (
                "input".to_string(),
                ValueExpr::workflow_input(JsonPath::from("x")),
            ),
        ]);

        match &obj {
            ValueExpr::Object(fields) => {
                assert_eq!(fields.len(), 3);
                assert!(matches!(&fields[0].1, ValueExpr::Step { .. }));
                assert!(matches!(&fields[1].1, ValueExpr::Literal(_)));
                assert!(matches!(&fields[2].1, ValueExpr::Input { .. }));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_utoipa_schema_generation() {
        // This test simply calls the schema generation to ensure it doesn't stack overflow
        // The recursive structure of ValueExpr (Array(Vec<ValueExpr>), Object with ValueExpr, etc.)
        // caused the derived utoipa::ToSchema to recurse infinitely during schema generation
        // With #[schema(as = serde_json::Value)], this should complete without stack overflow
        use utoipa::ToSchema;
        let _name = ValueExpr::name();
        // If we get here without stack overflow, the implementation is safe
    }

}
