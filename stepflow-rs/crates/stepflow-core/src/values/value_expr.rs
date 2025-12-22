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

use bit_set::BitSet;

use super::{JsonPath, PathPart, StepContext, ValueRef};
use crate::FlowResult;
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
    Step { step: String, path: JsonPath },

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
    EscapedLiteral { literal: serde_json::Value },

    /// Conditional expression: `{ $if: <condition>, then: <expr>, else?: <expr> }`
    /// Returns `then` value if condition is truthy, otherwise `else` value (defaults to null)
    If {
        condition: Box<ValueExpr>,
        then: Box<ValueExpr>,
        else_expr: Option<Box<ValueExpr>>,
    },

    /// Coalesce: `{ $coalesce: [<expr1>, <expr2>, ...] }`
    /// Returns first non-skipped, non-null value from the list
    Coalesce { values: Vec<ValueExpr> },

    /// JSON array where each element can be an expression
    Array(Vec<ValueExpr>),

    /// JSON object where each value can be an expression
    /// Uses Vec instead of Map for efficiency and to enable hashability
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

    /// Returns the set of step indices needed to evaluate this expression.
    ///
    /// An empty set means the expression is ready to be fully resolved.
    /// This method evaluates lazily - for conditional expressions like `$if`,
    /// it only returns the condition's dependencies until the condition can
    /// be evaluated, then returns the appropriate branch's dependencies.
    ///
    /// # Arguments
    /// * `ctx` - Context providing step completion state and results
    ///
    /// # Example
    /// For `{ $if: { $step: foo }, then: { $step: bar }, else: { $step: baz } }`:
    /// 1. First call (foo not complete): returns `{foo_index}`
    /// 2. After foo completes (truthy): returns `{bar_index}`
    /// 3. After bar completes: returns `{}` (ready!)
    pub fn needed_steps(&self, ctx: &impl StepContext) -> BitSet {
        /// Collect needed steps into a mutable BitSet.
        /// Returns `true` if we should stop early (for short-circuit evaluation).
        fn collect<C: StepContext>(expr: &ValueExpr, ctx: &C, needed: &mut BitSet) -> bool {
            match expr {
                ValueExpr::Step { step, .. } => {
                    if let Some(idx) = ctx.step_index(step)
                        && !ctx.is_completed(idx)
                    {
                        needed.insert(idx);
                    }
                    false
                }

                ValueExpr::Input { .. } | ValueExpr::Variable { .. } => false,

                ValueExpr::Literal(_) | ValueExpr::EscapedLiteral { .. } => false,

                ValueExpr::If {
                    condition,
                    then,
                    else_expr,
                } => {
                    // First collect condition's needs
                    let before = needed.len();
                    collect(condition, ctx, needed);
                    if needed.len() > before {
                        // Condition has unmet dependencies - stop here
                        return true;
                    }

                    // Condition is ready - evaluate to determine which branch
                    let cond_result = condition.resolve(ctx);
                    if is_truthy(&cond_result) {
                        collect(then, ctx, needed)
                    } else if let Some(else_e) = else_expr {
                        collect(else_e, ctx, needed)
                    } else {
                        false
                    }
                }

                ValueExpr::Coalesce { values } => {
                    for value in values {
                        let before = needed.len();
                        collect(value, ctx, needed);
                        if needed.len() > before {
                            // This value has unmet dependencies - stop here
                            return true;
                        }

                        // Value is ready - evaluate to decide if we should continue
                        let result = value.resolve(ctx);
                        match &result {
                            FlowResult::Success(v) if !v.as_ref().is_null() => {
                                // Found non-null value - done
                                return true;
                            }
                            FlowResult::Failed(_) => {
                                // Error - done (will propagate during resolution)
                                return true;
                            }
                            _ => {
                                // Null or skipped - continue to next value
                                continue;
                            }
                        }
                    }
                    false
                }

                ValueExpr::Array(items) => {
                    for item in items {
                        collect(item, ctx, needed);
                    }
                    false
                }

                ValueExpr::Object(fields) => {
                    for (_, value) in fields {
                        collect(value, ctx, needed);
                    }
                    false
                }
            }
        }

        let mut needed = BitSet::new();
        collect(self, ctx, &mut needed);
        needed
    }

    /// Resolve this expression using the provided context.
    ///
    /// This should only be called when `needed_steps()` returns an empty set,
    /// meaning all required step results are available in the context.
    ///
    /// The context provides access to:
    /// - Step results (`$step` references)
    /// - Workflow input (`$input` references)
    /// - Variables (`$variable` references)
    pub fn resolve(&self, ctx: &impl StepContext) -> FlowResult {
        match self {
            ValueExpr::Step { step, path } => {
                let Some(idx) = ctx.step_index(step) else {
                    return FlowResult::Failed(crate::FlowError::new(
                        404,
                        format!("Unknown step: {}", step),
                    ));
                };

                let Some(result) = ctx.get_result(idx) else {
                    return FlowResult::Failed(crate::FlowError::new(
                        500,
                        format!("Step {} not completed", step),
                    ));
                };

                // Apply path if needed
                match result {
                    // If the step returned null, propagate null (even if path is specified)
                    // This enables $coalesce to work with skipped steps that return null
                    FlowResult::Success(value) if value.as_ref().is_null() => {
                        FlowResult::Success(value.clone())
                    }
                    FlowResult::Success(value) if !path.is_empty() => {
                        if let Some(sub_value) = value.resolve_json_path(path) {
                            FlowResult::Success(sub_value)
                        } else {
                            FlowResult::Failed(crate::FlowError::new(
                                crate::FLOW_ERROR_UNDEFINED_FIELD,
                                format!("Path {} not found", path),
                            ))
                        }
                    }
                    other => other.clone(),
                }
            }

            ValueExpr::Input { input: path } => {
                let Some(input_value) = ctx.get_input() else {
                    return FlowResult::Failed(crate::FlowError::new(
                        500,
                        "Workflow input not available in context",
                    ));
                };

                // Apply path if provided
                if path.is_empty() {
                    FlowResult::Success(input_value.clone())
                } else if let Some(sub_value) = input_value.resolve_json_path(path) {
                    FlowResult::Success(sub_value)
                } else {
                    FlowResult::Failed(crate::FlowError::new(
                        crate::FLOW_ERROR_UNDEFINED_FIELD,
                        format!("Input path {} not found", path),
                    ))
                }
            }

            ValueExpr::Variable { variable, default } => {
                // Parse the variable name and path from the JsonPath
                let parts = variable.parts();
                if parts.is_empty() {
                    return FlowResult::Failed(crate::FlowError::new(
                        500,
                        "Variable path is empty",
                    ));
                }

                // First part is the variable name
                let var_name = match &parts[0] {
                    PathPart::Field(name) | PathPart::IndexStr(name) => name.as_str(),
                    PathPart::Index(_) => {
                        return FlowResult::Failed(crate::FlowError::new(
                            500,
                            "Variable name must be a string",
                        ));
                    }
                };

                // Try to get the variable value
                if let Some(var_value) = ctx.get_variable(var_name) {
                    // Apply remaining path if any
                    if parts.len() > 1 {
                        let sub_path = JsonPath::from_parts(parts[1..].to_vec());
                        if let Some(sub_value) = var_value.resolve_json_path(&sub_path) {
                            return FlowResult::Success(sub_value);
                        } else {
                            // Path not found - try default
                        }
                    } else {
                        return FlowResult::Success(var_value);
                    }
                }

                // Variable not found or path failed - try default if available
                if let Some(default_expr) = default {
                    log::debug!("Variable '{}' not found, using default", var_name);
                    return default_expr.resolve(ctx);
                }

                // No variable and no default - error
                FlowResult::Failed(crate::FlowError::new(
                    crate::FLOW_ERROR_UNDEFINED_FIELD,
                    format!("Undefined variable: {}", var_name),
                ))
            }

            ValueExpr::Literal(value) => FlowResult::Success(ValueRef::new(value.clone())),

            ValueExpr::EscapedLiteral { literal } => {
                FlowResult::Success(ValueRef::new(literal.clone()))
            }

            ValueExpr::If {
                condition,
                then,
                else_expr,
            } => {
                let cond_result = condition.resolve(ctx);
                if is_truthy(&cond_result) {
                    then.resolve(ctx)
                } else if let Some(else_e) = else_expr {
                    else_e.resolve(ctx)
                } else {
                    FlowResult::Success(ValueRef::new(serde_json::Value::Null))
                }
            }

            ValueExpr::Coalesce { values } => {
                for value in values {
                    let result = value.resolve(ctx);
                    match &result {
                        FlowResult::Success(v) if !v.as_ref().is_null() => {
                            return result;
                        }
                        FlowResult::Failed(_) => {
                            return result;
                        }
                        _ => continue,
                    }
                }
                FlowResult::Success(ValueRef::new(serde_json::Value::Null))
            }

            ValueExpr::Array(items) => {
                let mut result_array = Vec::new();
                for item in items {
                    match item.resolve(ctx) {
                        FlowResult::Success(value) => {
                            result_array.push(value.as_ref().clone());
                        }
                        other => return other,
                    }
                }
                FlowResult::Success(ValueRef::new(serde_json::Value::Array(result_array)))
            }

            ValueExpr::Object(fields) => {
                let mut result_map = serde_json::Map::new();
                for (k, v) in fields {
                    match v.resolve(ctx) {
                        FlowResult::Success(value) => {
                            result_map.insert(k.clone(), value.as_ref().clone());
                        }
                        other => return other,
                    }
                }
                FlowResult::Success(ValueRef::new(serde_json::Value::Object(result_map)))
            }
        }
    }
}

/// Check if a FlowResult is truthy for conditional evaluation.
///
/// - `Success` with non-null, non-false value is truthy
/// - `Success` with null or false is falsy
/// - `Failed` is treated as falsy (condition evaluation failed)
fn is_truthy(result: &FlowResult) -> bool {
    match result {
        FlowResult::Success(value) => match value.as_ref() {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(b) => *b,
            _ => true,
        },
        FlowResult::Failed(_) => false,
    }
}

impl Default for ValueExpr {
    fn default() -> Self {
        ValueExpr::null()
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

        // Helper for creating a string type schema
        let string_type = || {
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .schema_type(SchemaType::Type(Type::String))
                    .build(),
            ))
        };

        // Helper for creating a string type with description
        let string_type_with_desc = |desc: &'static str| {
            RefOr::T(Schema::Object(
                ObjectBuilder::new()
                    .schema_type(SchemaType::Type(Type::String))
                    .description(Some(desc))
                    .build(),
            ))
        };

        // Create a oneOf schema with each variant described
        let one_of_items = vec![
            // Step reference: { $step: "step_id", path?: "..." }
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("StepRef"))
                    .property("$step", string_type())
                    .property("path", string_type_with_desc("JSONPath expression"))
                    .required("$step")
                    .description(Some(
                        "Step reference: { $step: \"step_id\", path?: \"...\" }",
                    ))
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .build(),
            ),
            // Input reference: { $input: "path" }
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("InputRef"))
                    .property("$input", string_type_with_desc("JSONPath expression"))
                    .required("$input")
                    .description(Some("Workflow input reference: { $input: \"path\" }"))
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .build(),
            ),
            // Variable reference: { $variable: "path", default?: ValueExpr }
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("VariableRef"))
                    .property(
                        "$variable",
                        string_type_with_desc("JSONPath expression including variable name"),
                    )
                    .property(
                        "default",
                        RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")),
                    )
                    .required("$variable")
                    .description(Some(
                        "Variable reference: { $variable: \"path\", default?: ValueExpr }",
                    ))
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .build(),
            ),
            // Escaped literal: { $literal: any }
            // Note: Named "LiteralExpr" to avoid conflict with Python's typing.Literal
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("LiteralExpr"))
                    .property(
                        "$literal",
                        // Empty AllOf means "any value" (produces {} in JSON)
                        Schema::AllOf(AllOfBuilder::new().build()),
                    )
                    .required("$literal")
                    .description(Some("Escaped literal: { $literal: any }"))
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .build(),
            ),
            // Conditional: { $if: condition, then: expr, else?: expr }
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("If"))
                    .property(
                        "$if",
                        RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")),
                    )
                    .property(
                        "then",
                        RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")),
                    )
                    .property(
                        "else",
                        RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")),
                    )
                    .required("$if")
                    .required("then")
                    .description(Some(
                        "Conditional: { $if: condition, then: expr, else?: expr }",
                    ))
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .build(),
            ),
            // Coalesce: { $coalesce: [expr1, expr2, ...] }
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("Coalesce"))
                    .property(
                        "$coalesce",
                        ArrayBuilder::new()
                            .items(RefOr::Ref(Ref::new("#/components/schemas/ValueExpr"))),
                    )
                    .required("$coalesce")
                    .description(Some("Coalesce: { $coalesce: [expr1, expr2, ...] }"))
                    .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                    .build(),
            ),
            // Array of expressions
            Schema::Array(
                ArrayBuilder::new()
                    .title(Some("ArrayExpr"))
                    .items(RefOr::Ref(Ref::new("#/components/schemas/ValueExpr")))
                    .description(Some("Array of expressions"))
                    .build(),
            ),
            // Object with expression values
            Schema::Object(
                ObjectBuilder::new()
                    .title(Some("ObjectExpr"))
                    .additional_properties(Some(RefOr::Ref(Ref::new(
                        "#/components/schemas/ValueExpr",
                    ))))
                    .description(Some("Object with expression values"))
                    .build(),
            ),
            // Literal primitive value - oneOf for null, boolean, number, string
            Schema::OneOf(
                OneOfBuilder::new()
                    .title(Some("PrimitiveValue"))
                    .description(Some("Literal primitive value"))
                    .item(Schema::Object(
                        ObjectBuilder::new()
                            .schema_type(SchemaType::Type(Type::Null))
                            .build(),
                    ))
                    .item(Schema::Object(
                        ObjectBuilder::new()
                            .schema_type(SchemaType::Type(Type::Boolean))
                            .build(),
                    ))
                    .item(Schema::Object(
                        ObjectBuilder::new()
                            .schema_type(SchemaType::Type(Type::Number))
                            .build(),
                    ))
                    .item(Schema::Object(
                        ObjectBuilder::new()
                            .schema_type(SchemaType::Type(Type::String))
                            .build(),
                    ))
                    .build(),
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
    use crate::values::Secrets;
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

    // ========== Tests for needed_steps() ==========

    /// Mock StepContext for testing
    struct MockStepContext {
        step_names: Vec<String>,
        completed: BitSet,
        results: Vec<Option<FlowResult>>,
        input: Option<ValueRef>,
    }

    impl MockStepContext {
        fn new(step_names: Vec<&str>) -> Self {
            let len = step_names.len();
            Self {
                step_names: step_names.into_iter().map(|s| s.to_string()).collect(),
                completed: BitSet::new(),
                results: vec![None; len],
                input: None,
            }
        }

        #[allow(dead_code)]
        fn with_input(step_names: Vec<&str>, input: serde_json::Value) -> Self {
            let mut ctx = Self::new(step_names);
            ctx.input = Some(ValueRef::new(input));
            ctx
        }

        fn complete_step(&mut self, name: &str, result: FlowResult) {
            if let Some(idx) = self.step_names.iter().position(|s| s == name) {
                self.completed.insert(idx);
                self.results[idx] = Some(result);
            }
        }
    }

    impl StepContext for MockStepContext {
        fn step_index(&self, step_id: &str) -> Option<usize> {
            self.step_names.iter().position(|s| s == step_id)
        }

        fn is_completed(&self, step_index: usize) -> bool {
            self.completed.contains(step_index)
        }

        fn get_result(&self, step_index: usize) -> Option<&FlowResult> {
            self.results.get(step_index).and_then(|r| r.as_ref())
        }

        fn get_input(&self) -> Option<&ValueRef> {
            self.input.as_ref()
        }

        fn get_variable(&self, _name: &str) -> Option<ValueRef> {
            None // Mock doesn't support variables
        }

        fn get_variable_secrets(&self, _name: &str) -> Secrets {
            Secrets::empty().clone()
        }
    }

    #[test]
    fn test_needed_steps_literal() {
        let ctx = MockStepContext::new(vec!["step1"]);
        let expr = ValueExpr::literal(json!(42));
        let needs = expr.needed_steps(&ctx);
        assert!(needs.is_empty(), "Literals should need no steps");
    }

    #[test]
    fn test_needed_steps_step_not_completed() {
        let ctx = MockStepContext::new(vec!["step1", "step2"]);
        let expr = ValueExpr::step("step1", JsonPath::default());
        let needs = expr.needed_steps(&ctx);
        assert!(needs.contains(0), "Should need step1 (index 0)");
        assert!(!needs.contains(1), "Should not need step2");
    }

    #[test]
    fn test_needed_steps_step_completed() {
        let mut ctx = MockStepContext::new(vec!["step1"]);
        ctx.complete_step("step1", FlowResult::Success(ValueRef::new(json!(42))));

        let expr = ValueExpr::step("step1", JsonPath::default());
        let needs = expr.needed_steps(&ctx);
        assert!(needs.is_empty(), "Completed step should need nothing");
    }

    #[test]
    fn test_needed_steps_if_condition_not_ready() {
        let ctx = MockStepContext::new(vec!["cond", "then_step", "else_step"]);

        // { $if: { $step: cond }, then: { $step: then_step }, else: { $step: else_step } }
        let expr = ValueExpr::if_expr(
            ValueExpr::step("cond", JsonPath::default()),
            ValueExpr::step("then_step", JsonPath::default()),
            Some(ValueExpr::step("else_step", JsonPath::default())),
        );

        let needs = expr.needed_steps(&ctx);
        assert!(needs.contains(0), "Should need condition step");
        assert!(!needs.contains(1), "Should NOT need then_step yet");
        assert!(!needs.contains(2), "Should NOT need else_step yet");
    }

    #[test]
    fn test_needed_steps_if_condition_true() {
        let mut ctx = MockStepContext::new(vec!["cond", "then_step", "else_step"]);
        ctx.complete_step("cond", FlowResult::Success(ValueRef::new(json!(true))));

        let expr = ValueExpr::if_expr(
            ValueExpr::step("cond", JsonPath::default()),
            ValueExpr::step("then_step", JsonPath::default()),
            Some(ValueExpr::step("else_step", JsonPath::default())),
        );

        let needs = expr.needed_steps(&ctx);
        assert!(!needs.contains(0), "Should not need condition (completed)");
        assert!(
            needs.contains(1),
            "Should need then_step (condition was true)"
        );
        assert!(!needs.contains(2), "Should NOT need else_step");
    }

    #[test]
    fn test_needed_steps_if_condition_false() {
        let mut ctx = MockStepContext::new(vec!["cond", "then_step", "else_step"]);
        ctx.complete_step("cond", FlowResult::Success(ValueRef::new(json!(false))));

        let expr = ValueExpr::if_expr(
            ValueExpr::step("cond", JsonPath::default()),
            ValueExpr::step("then_step", JsonPath::default()),
            Some(ValueExpr::step("else_step", JsonPath::default())),
        );

        let needs = expr.needed_steps(&ctx);
        assert!(!needs.contains(0), "Should not need condition (completed)");
        assert!(!needs.contains(1), "Should NOT need then_step");
        assert!(
            needs.contains(2),
            "Should need else_step (condition was false)"
        );
    }

    #[test]
    fn test_needed_steps_if_fully_ready() {
        let mut ctx = MockStepContext::new(vec!["cond", "then_step", "else_step"]);
        ctx.complete_step("cond", FlowResult::Success(ValueRef::new(json!(true))));
        ctx.complete_step(
            "then_step",
            FlowResult::Success(ValueRef::new(json!("result"))),
        );

        let expr = ValueExpr::if_expr(
            ValueExpr::step("cond", JsonPath::default()),
            ValueExpr::step("then_step", JsonPath::default()),
            Some(ValueExpr::step("else_step", JsonPath::default())),
        );

        let needs = expr.needed_steps(&ctx);
        assert!(needs.is_empty(), "All needed steps completed - ready");
    }

    #[test]
    fn test_needed_steps_coalesce_first_value_not_ready() {
        let ctx = MockStepContext::new(vec!["step1", "step2"]);

        let expr = ValueExpr::coalesce(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::step("step2", JsonPath::default()),
        ]);

        let needs = expr.needed_steps(&ctx);
        assert!(needs.contains(0), "Should need step1 first");
        assert!(!needs.contains(1), "Should NOT need step2 yet");
    }

    #[test]
    fn test_needed_steps_coalesce_first_value_null() {
        let mut ctx = MockStepContext::new(vec!["step1", "step2"]);
        ctx.complete_step("step1", FlowResult::Success(ValueRef::new(json!(null))));

        let expr = ValueExpr::coalesce(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::step("step2", JsonPath::default()),
        ]);

        let needs = expr.needed_steps(&ctx);
        assert!(!needs.contains(0), "step1 completed (null)");
        assert!(needs.contains(1), "Should now need step2");
    }

    #[test]
    fn test_needed_steps_coalesce_first_value_success() {
        let mut ctx = MockStepContext::new(vec!["step1", "step2"]);
        ctx.complete_step("step1", FlowResult::Success(ValueRef::new(json!("value"))));

        let expr = ValueExpr::coalesce(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::step("step2", JsonPath::default()),
        ]);

        let needs = expr.needed_steps(&ctx);
        assert!(needs.is_empty(), "Found non-null - no more steps needed");
    }

    #[test]
    fn test_needed_steps_coalesce_null_continues() {
        let mut ctx = MockStepContext::new(vec!["step1", "step2"]);
        // Step completed with null value (equivalent to old "skipped")
        ctx.complete_step("step1", FlowResult::Success(ValueRef::new(json!(null))));

        let expr = ValueExpr::coalesce(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::step("step2", JsonPath::default()),
        ]);

        let needs = expr.needed_steps(&ctx);
        assert!(!needs.contains(0), "step1 completed (null)");
        assert!(
            needs.contains(1),
            "Should now need step2 (coalesce continues on null)"
        );
    }

    #[test]
    fn test_needed_steps_array_union() {
        let ctx = MockStepContext::new(vec!["step1", "step2", "step3"]);

        let expr = ValueExpr::array(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::step("step2", JsonPath::default()),
            ValueExpr::literal(json!("literal")),
        ]);

        let needs = expr.needed_steps(&ctx);
        assert!(needs.contains(0), "Should need step1");
        assert!(needs.contains(1), "Should need step2");
        assert!(!needs.contains(2), "Should not need step3 (not referenced)");
    }

    #[test]
    fn test_needed_steps_object_union() {
        let ctx = MockStepContext::new(vec!["step1", "step2"]);

        let expr = ValueExpr::object(vec![
            (
                "a".to_string(),
                ValueExpr::step("step1", JsonPath::default()),
            ),
            (
                "b".to_string(),
                ValueExpr::step("step2", JsonPath::default()),
            ),
        ]);

        let needs = expr.needed_steps(&ctx);
        assert!(needs.contains(0), "Should need step1");
        assert!(needs.contains(1), "Should need step2");
    }

    #[test]
    fn test_needed_steps_nested_if_in_coalesce() {
        let ctx = MockStepContext::new(vec!["cond", "then_step", "fallback"]);

        // { $coalesce: [{ $if: { $step: cond }, then: { $step: then_step } }, { $step: fallback }] }
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::if_expr(
                ValueExpr::step("cond", JsonPath::default()),
                ValueExpr::step("then_step", JsonPath::default()),
                None,
            ),
            ValueExpr::step("fallback", JsonPath::default()),
        ]);

        // First: need condition for the $if
        let needs = expr.needed_steps(&ctx);
        assert!(needs.contains(0), "Should need cond first");
        assert!(!needs.contains(1), "Should NOT need then_step yet");
        assert!(!needs.contains(2), "Should NOT need fallback yet");
    }

    #[test]
    fn test_needed_steps_nested_if_condition_false_returns_null() {
        let mut ctx = MockStepContext::new(vec!["cond", "then_step", "fallback"]);
        ctx.complete_step("cond", FlowResult::Success(ValueRef::new(json!(false))));

        // $if with no else returns null when condition is false
        let expr = ValueExpr::coalesce(vec![
            ValueExpr::if_expr(
                ValueExpr::step("cond", JsonPath::default()),
                ValueExpr::step("then_step", JsonPath::default()),
                None, // No else - returns null
            ),
            ValueExpr::step("fallback", JsonPath::default()),
        ]);

        let needs = expr.needed_steps(&ctx);
        // Condition is false, $if returns null, coalesce moves to fallback
        assert!(!needs.contains(0), "Condition completed");
        assert!(!needs.contains(1), "then_step not needed (condition false)");
        assert!(needs.contains(2), "Should need fallback now");
    }

    #[test]
    fn test_resolve_literal() {
        let ctx = MockStepContext::new(vec![]);
        let expr = ValueExpr::literal(json!(42));
        let result = expr.resolve(&ctx);
        assert_eq!(result.success().unwrap().as_ref(), &json!(42));
    }

    #[test]
    fn test_resolve_step() {
        let mut ctx = MockStepContext::new(vec!["step1"]);
        ctx.complete_step(
            "step1",
            FlowResult::Success(ValueRef::new(json!({"value": 42}))),
        );

        let expr = ValueExpr::step("step1", JsonPath::default());
        let result = expr.resolve(&ctx);
        assert_eq!(result.success().unwrap().as_ref(), &json!({"value": 42}));
    }

    #[test]
    fn test_resolve_step_with_path() {
        let mut ctx = MockStepContext::new(vec!["step1"]);
        ctx.complete_step(
            "step1",
            FlowResult::Success(ValueRef::new(json!({"value": 42}))),
        );

        let expr = ValueExpr::step("step1", JsonPath::from("value"));
        let result = expr.resolve(&ctx);
        assert_eq!(result.success().unwrap().as_ref(), &json!(42));
    }

    #[test]
    fn test_resolve_if_true() {
        let mut ctx = MockStepContext::new(vec!["cond", "then_step"]);
        ctx.complete_step("cond", FlowResult::Success(ValueRef::new(json!(true))));
        ctx.complete_step(
            "then_step",
            FlowResult::Success(ValueRef::new(json!("then_value"))),
        );

        let expr = ValueExpr::if_expr(
            ValueExpr::step("cond", JsonPath::default()),
            ValueExpr::step("then_step", JsonPath::default()),
            Some(ValueExpr::literal(json!("else_value"))),
        );

        let result = expr.resolve(&ctx);
        assert_eq!(result.success().unwrap().as_ref(), &json!("then_value"));
    }

    #[test]
    fn test_resolve_if_false() {
        let mut ctx = MockStepContext::new(vec!["cond"]);
        ctx.complete_step("cond", FlowResult::Success(ValueRef::new(json!(false))));

        let expr = ValueExpr::if_expr(
            ValueExpr::step("cond", JsonPath::default()),
            ValueExpr::literal(json!("then_value")),
            Some(ValueExpr::literal(json!("else_value"))),
        );

        let result = expr.resolve(&ctx);
        assert_eq!(result.success().unwrap().as_ref(), &json!("else_value"));
    }

    #[test]
    fn test_resolve_coalesce() {
        let mut ctx = MockStepContext::new(vec!["step1", "step2"]);
        ctx.complete_step("step1", FlowResult::Success(ValueRef::new(json!(null))));
        ctx.complete_step("step2", FlowResult::Success(ValueRef::new(json!("value"))));

        let expr = ValueExpr::coalesce(vec![
            ValueExpr::step("step1", JsonPath::default()),
            ValueExpr::step("step2", JsonPath::default()),
        ]);

        let result = expr.resolve(&ctx);
        assert_eq!(result.success().unwrap().as_ref(), &json!("value"));
    }

    #[test]
    fn test_is_truthy() {
        // Truthy values
        assert!(is_truthy(&FlowResult::Success(ValueRef::new(json!(true)))));
        assert!(is_truthy(&FlowResult::Success(ValueRef::new(json!(1)))));
        assert!(is_truthy(&FlowResult::Success(ValueRef::new(json!("str")))));
        assert!(is_truthy(&FlowResult::Success(ValueRef::new(json!([])))));
        assert!(is_truthy(&FlowResult::Success(ValueRef::new(json!({})))));

        // Falsy values
        assert!(!is_truthy(&FlowResult::Success(ValueRef::new(json!(null)))));
        assert!(!is_truthy(&FlowResult::Success(ValueRef::new(json!(
            false
        )))));
        assert!(!is_truthy(&FlowResult::Failed(crate::FlowError::new(
            500, "error"
        ))));
    }
}
