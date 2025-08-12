// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::sync::Arc;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ValueRef;
use crate::workflow::{Expr, JsonPath};

/// Internal representation of a value template that can contain expressions or literal JSON values.
///
/// This enum mirrors `serde_json::Value` but adds support for expressions with `$from` syntax.
/// It is wrapped in `ValueTemplate` to provide efficient reference semantics.
///
/// Uses untagged serialization to produce natural JSON, but custom deserialization
/// to avoid trial-and-error parsing issues.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
#[schemars(
    description = "A value that can be either a literal JSON value or an expression that references other values using the $from syntax"
)]
pub enum ValueTemplateRepr {
    /// An expression with `$from` syntax for referencing other values
    Expression(Expr),

    /// JSON null value
    Null,

    /// JSON boolean value
    Bool(bool),

    /// JSON numeric value
    Number(serde_json::Number),

    /// JSON string value
    String(String),

    /// JSON array where each element can be a template
    Array(Vec<ValueTemplate>),

    /// JSON object where each value can be a template
    Object(IndexMap<String, ValueTemplate>),
}

/// A value that can contain literal JSON data or template expressions.
///
/// This type represents JSON values that may contain `$from` expressions for
/// referencing other values in a workflow. It provides efficient reference
/// semantics through an Arc wrapper to avoid excessive cloning.
///
/// Unlike `ValueRef` which is purely for literal JSON values, `ValueTemplate`
/// explicitly supports templating with expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ValueTemplate(Arc<ValueTemplateRepr>);

impl JsonSchema for ValueTemplate {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ValueTemplate".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ValueTemplateRepr::json_schema(generator)
    }
}

impl ValueTemplate {
    /// Create a null value template
    pub fn null() -> Self {
        Self(Arc::new(ValueTemplateRepr::Null))
    }

    /// Create a boolean value template
    pub fn bool(value: bool) -> Self {
        Self(Arc::new(ValueTemplateRepr::Bool(value)))
    }

    /// Create a numeric value template
    pub fn number(value: impl Into<serde_json::Number>) -> Self {
        Self(Arc::new(ValueTemplateRepr::Number(value.into())))
    }

    /// Create a string value template
    pub fn string(value: impl Into<String>) -> Self {
        Self(Arc::new(ValueTemplateRepr::String(value.into())))
    }

    /// Create an array value template
    pub fn array(values: Vec<ValueTemplate>) -> Self {
        Self(Arc::new(ValueTemplateRepr::Array(values)))
    }

    /// Create an object value template
    pub fn object(values: IndexMap<String, ValueTemplate>) -> Self {
        Self(Arc::new(ValueTemplateRepr::Object(values)))
    }

    pub fn empty_object() -> Self {
        Self(Arc::new(ValueTemplateRepr::Object(IndexMap::new())))
    }

    /// Create an expression value template
    pub fn expression(expr: Expr) -> Self {
        Self(Arc::new(ValueTemplateRepr::Expression(expr)))
    }

    /// Create a value template from a JSON value (treating it as literal data)
    pub fn literal(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::null(),
            serde_json::Value::Bool(b) => Self::bool(b),
            serde_json::Value::Number(n) => Self::number(n),
            serde_json::Value::String(s) => Self::string(s),
            serde_json::Value::Array(arr) => {
                let templates = arr.into_iter().map(Self::literal).collect();
                Self::array(templates)
            }
            serde_json::Value::Object(obj) => {
                let templates = obj
                    .into_iter()
                    .map(|(k, v)| (k, Self::literal(v)))
                    .collect();
                Self::object(templates)
            }
        }
    }

    /// Create a step reference template
    /// - `step_ref("step1", JsonPath::default())` creates `{"$from": {"step": "step1"}}`
    /// - `step_ref("step1", JsonPath::from("field"))` creates `{"$from": {"step": "step1"}, "path": "field"}`
    pub fn step_ref(step_id: impl Into<String>, path: JsonPath) -> Self {
        Self::expression(Expr::step_ref(step_id, path))
    }

    /// Create a workflow input reference template
    /// - `workflow_input(JsonPath::default())` creates `{"$from": {"workflow": "input"}}`
    /// - `workflow_input(JsonPath::from("field"))` creates `{"$from": {"workflow": "input"}, "path": "field"}`
    pub fn workflow_input(path: JsonPath) -> Self {
        Self::expression(Expr::workflow_input(path))
    }

    /// Parse a JSON value as a template, interpreting expressions like `{"$from": ...}`
    ///
    /// This method will parse `{"$from": {"step": "step1"}}` as a step reference expression.
    pub fn parse_value(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn is_null(&self) -> bool {
        matches!(self.as_ref(), ValueTemplateRepr::Null)
    }

    /// Check if this template contains only literal values (no expressions)
    pub fn is_literal(&self) -> bool {
        self.expressions().next().is_none() // If there are no expressions, it's literal
    }

    /// Iterate over all expressions contained within this template.
    ///
    /// This performs a tree traversal to find all `Expr` instances, allowing
    /// analysis code to examine the expressions without duplicating traversal logic.
    pub fn expressions(&self) -> ExpressionIterator<'_> {
        ExpressionIterator::new(self)
    }
}

/// Iterator over all expressions contained within a ValueTemplate.
///
/// This performs a depth-first traversal of the template structure to yield
/// all `Expr` instances found within the template.
pub struct ExpressionIterator<'a> {
    stack: Vec<&'a ValueTemplate>,
}

impl<'a> ExpressionIterator<'a> {
    fn new(template: &'a ValueTemplate) -> Self {
        Self {
            stack: vec![template],
        }
    }
}

impl<'a> Iterator for ExpressionIterator<'a> {
    type Item = &'a Expr;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(template) = self.stack.pop() {
            match template.as_ref() {
                ValueTemplateRepr::Expression(expr) => {
                    // For expressions, return them normally.
                    // Note: This does not recurse into expressions within an escaped literal.
                    return Some(expr);
                }
                ValueTemplateRepr::Object(obj) => {
                    // Add all object values to stack for traversal
                    self.stack.extend(obj.values());
                }
                ValueTemplateRepr::Array(arr) => {
                    // Add all array elements to stack for traversal
                    self.stack.extend(arr.iter());
                }
                // Literal values don't contain expressions
                _ => continue,
            }
        }
        None
    }
}

impl Default for ValueTemplate {
    fn default() -> Self {
        Self::null()
    }
}

impl AsRef<ValueTemplateRepr> for ValueTemplate {
    fn as_ref(&self) -> &ValueTemplateRepr {
        &self.0
    }
}

// Custom deserialization for ValueTemplateRepr

impl<'de> Deserialize<'de> for ValueTemplateRepr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        // Deserialize to a raw serde_json::Value first
        let value = serde_json::Value::deserialize(deserializer)?;

        // Check if it's an expression by looking for $from or $literal
        if let serde_json::Value::Object(ref map) = value {
            if map.contains_key("$from") || map.contains_key("$literal") {
                // Try to deserialize as an expression - must succeed or it's an error
                let expr = Expr::deserialize(value).map_err(D::Error::custom)?;
                return Ok(ValueTemplateRepr::Expression(expr));
            }
        }

        // Otherwise, convert the JSON value to our template representation
        let template_repr = json_value_to_template_repr(value).map_err(D::Error::custom)?;
        Ok(template_repr)
    }
}

/// Convert a serde_json::Value to ValueTemplateRepr recursively
fn json_value_to_template_repr(value: serde_json::Value) -> Result<ValueTemplateRepr, String> {
    match value {
        serde_json::Value::Null => Ok(ValueTemplateRepr::Null),
        serde_json::Value::Bool(b) => Ok(ValueTemplateRepr::Bool(b)),
        serde_json::Value::Number(n) => Ok(ValueTemplateRepr::Number(n)),
        serde_json::Value::String(s) => Ok(ValueTemplateRepr::String(s)),
        serde_json::Value::Array(arr) => {
            let templates = arr
                .into_iter()
                .map(|v| json_value_to_template_repr(v).map(|repr| ValueTemplate(Arc::new(repr))))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ValueTemplateRepr::Array(templates))
        }
        serde_json::Value::Object(obj) => {
            // Check if this object is an expression
            if obj.contains_key("$from") || obj.contains_key("$literal") {
                // Try to deserialize as an expression - must succeed or it's an error
                let expr = Expr::deserialize(serde_json::Value::Object(obj.clone()))
                    .map_err(|e| format!("Invalid expression: {e}"))?;
                return Ok(ValueTemplateRepr::Expression(expr));
            }

            // Convert to regular object
            let templates = obj
                .into_iter()
                .map(|(k, v)| {
                    json_value_to_template_repr(v).map(|repr| (k, ValueTemplate(Arc::new(repr))))
                })
                .collect::<Result<IndexMap<_, _>, _>>()?;
            Ok(ValueTemplateRepr::Object(templates))
        }
    }
}

// ValueTemplate uses #[serde(transparent)] for automatic delegation

// Conversion traits

impl From<ValueRef> for ValueTemplate {
    fn from(value_ref: ValueRef) -> Self {
        // ValueRef implements Serialize, so we can convert it to serde_json::Value
        let json_value =
            serde_json::to_value(&value_ref).expect("ValueRef should always be valid JSON");
        Self::literal(json_value)
    }
}

impl From<Expr> for ValueTemplate {
    fn from(expr: Expr) -> Self {
        Self::expression(expr)
    }
}

impl From<bool> for ValueTemplate {
    fn from(value: bool) -> Self {
        Self::bool(value)
    }
}

impl From<i32> for ValueTemplate {
    fn from(value: i32) -> Self {
        Self::number(value)
    }
}

impl From<i64> for ValueTemplate {
    fn from(value: i64) -> Self {
        Self::number(value)
    }
}

impl From<f64> for ValueTemplate {
    fn from(value: f64) -> Self {
        match serde_json::Number::from_f64(value) {
            Some(num) => Self::number(num),
            None => Self::null(), // Invalid f64 becomes null
        }
    }
}

impl From<String> for ValueTemplate {
    fn from(value: String) -> Self {
        Self::string(value)
    }
}

impl From<&str> for ValueTemplate {
    fn from(value: &str) -> Self {
        Self::string(value)
    }
}

impl<T: Into<ValueTemplate>> From<Vec<T>> for ValueTemplate {
    fn from(values: Vec<T>) -> Self {
        let templates = values.into_iter().map(|v| v.into()).collect();
        Self::array(templates)
    }
}

impl<T: Into<ValueTemplate>> From<IndexMap<String, T>> for ValueTemplate {
    fn from(values: IndexMap<String, T>) -> Self {
        let templates = values.into_iter().map(|(k, v)| (k, v.into())).collect();
        Self::object(templates)
    }
}

impl utoipa::PartialSchema for ValueTemplate {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::{AllOfBuilder, RefOr, schema::Schema};

        RefOr::T(Schema::AllOf(
            AllOfBuilder::new()
                .description(Some(
                    "A value that can be either a literal JSON value or an expression that references other values using the $from syntax",
                ))
                .build(),
        ))
    }
}

impl utoipa::ToSchema for ValueTemplate {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{BaseRef, Expr, WorkflowRef};

    #[test]
    fn test_literal_constructors() {
        assert!(ValueTemplate::null().is_literal());
        assert!(ValueTemplate::bool(true).is_literal());
        assert!(ValueTemplate::string("hello").is_literal());
        assert!(ValueTemplate::number(42).is_literal());
    }

    #[test]
    fn test_expression_constructor() {
        let expr = Expr::literal(serde_json::json!("test"));
        let template = ValueTemplate::expression(expr);
        assert!(!template.is_literal());
    }

    #[test]
    fn test_array_literal_check() {
        let literal_array = ValueTemplate::array(vec![
            ValueTemplate::string("hello"),
            ValueTemplate::number(42),
        ]);
        assert!(literal_array.is_literal());

        let expr = Expr::literal(serde_json::json!("test"));
        let template_array = ValueTemplate::array(vec![
            ValueTemplate::string("hello"),
            ValueTemplate::expression(expr),
        ]);
        assert!(!template_array.is_literal());
    }

    #[test]
    fn test_object_literal_check() {
        let mut obj = IndexMap::new();
        obj.insert("key1".to_string(), ValueTemplate::string("value1"));
        obj.insert("key2".to_string(), ValueTemplate::number(42));
        let literal_object = ValueTemplate::object(obj);
        assert!(literal_object.is_literal());

        let mut obj_with_expr = IndexMap::new();
        obj_with_expr.insert("key1".to_string(), ValueTemplate::string("value1"));
        let expr = Expr::literal(serde_json::json!("test"));
        obj_with_expr.insert("key2".to_string(), ValueTemplate::expression(expr));
        let template_object = ValueTemplate::object(obj_with_expr);
        assert!(!template_object.is_literal());
    }

    #[test]
    fn test_literal() {
        let json_val = serde_json::json!({
            "string": "hello",
            "number": 42,
            "bool": true,
            "null": null,
            "array": [1, 2, 3],
            "object": {"nested": "value"}
        });

        let template = ValueTemplate::literal(json_val);
        assert!(template.is_literal());
    }

    #[test]
    fn test_conversions() {
        // Test basic type conversions
        let bool_template: ValueTemplate = true.into();
        assert!(bool_template.is_literal());

        let int_template: ValueTemplate = 42i32.into();
        assert!(int_template.is_literal());

        let string_template: ValueTemplate = "hello".into();
        assert!(string_template.is_literal());

        // Test array conversion
        let arr_template: ValueTemplate = vec![1, 2, 3].into();
        assert!(arr_template.is_literal());

        // Test expression conversion
        let expr = Expr::literal(serde_json::json!("test"));
        let expr_template: ValueTemplate = expr.into();
        assert!(!expr_template.is_literal());
    }

    #[test]
    fn test_serialization_round_trip() {
        // Test literal values
        let literal_template = ValueTemplate::literal(serde_json::json!({
            "string": "hello",
            "number": 42,
            "bool": true,
            "null": null,
            "array": [1, 2, 3]
        }));

        let serialized = serde_json::to_string(&literal_template).unwrap();
        let deserialized: ValueTemplate = serde_json::from_str(&serialized).unwrap();
        assert_eq!(literal_template, deserialized);

        // Check that serialized form is natural JSON (not Rust enum)
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert!(parsed.is_object());
        assert_eq!(parsed["string"], "hello");
        assert_eq!(parsed["number"], 42);
    }

    #[test]
    fn test_expression_serialization() {
        // Create an expression template
        let expr = Expr::Ref {
            from: BaseRef::Workflow(WorkflowRef::Input),
            path: "name".into(),
            on_skip: Default::default(),
        };
        let expr_template = ValueTemplate::expression(expr);

        let serialized = serde_json::to_string(&expr_template).unwrap();
        let deserialized: ValueTemplate = serde_json::from_str(&serialized).unwrap();
        assert_eq!(expr_template, deserialized);

        // Check that expressions serialize with $from
        assert!(serialized.contains("$from"));
        assert!(serialized.contains("workflow"));
    }

    #[test]
    fn test_mixed_template_serialization() {
        // Create a template with both literals and expressions
        let expr = Expr::Ref {
            from: BaseRef::Workflow(WorkflowRef::Input),
            path: "name".into(),
            on_skip: Default::default(),
        };

        let mut obj = IndexMap::new();
        obj.insert("literal".to_string(), ValueTemplate::string("hello"));
        obj.insert("expression".to_string(), ValueTemplate::expression(expr));
        obj.insert("number".to_string(), ValueTemplate::number(42));

        let template = ValueTemplate::object(obj);

        let serialized = serde_json::to_string(&template).unwrap();
        let deserialized: ValueTemplate = serde_json::from_str(&serialized).unwrap();
        assert_eq!(template, deserialized);

        // Verify natural JSON structure
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["literal"], "hello");
        assert_eq!(parsed["number"], 42);
        assert!(parsed["expression"].is_object());
        assert!(parsed["expression"]["$from"].is_object());
    }

    #[test]
    fn test_literal_value_serialization() {
        // Test $literal syntax for escaping
        let literal_expr = Expr::escaped_literal(serde_json::json!({
            "$from": {"step": "some_step"},
            "other": "data"
        }));
        let template = ValueTemplate::expression(literal_expr);

        let serialized = serde_json::to_string(&template).unwrap();
        let deserialized: ValueTemplate = serde_json::from_str(&serialized).unwrap();
        assert_eq!(template, deserialized);

        // Check that it serializes with $literal
        assert!(serialized.contains("$literal"));
        assert!(serialized.contains("$from"));

        // Verify that the inner $from is NOT treated as an expression
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert!(parsed["$literal"].is_object());
        assert!(parsed["$literal"]["$from"].is_object());
    }

    #[test]
    fn test_literal_vs_expression_distinction() {
        // Test that $from creates an expression when deserialized
        let json_str = r#"{"$from": {"workflow": "input"}, "path": "name"}"#;
        let from_template: ValueTemplate = serde_json::from_str(json_str).unwrap();
        assert!(!from_template.is_literal());

        // Test that $literal creates an expression when deserialized
        let literal_json_str = r#"{"$literal": {"$from": {"step": "some_step"}, "data": "value"}}"#;
        let literal_template: ValueTemplate = serde_json::from_str(literal_json_str).unwrap();
        assert!(!literal_template.is_literal()); // Contains an expression (the $literal expression)

        // Test pure literals
        let pure_literal = ValueTemplate::literal(serde_json::json!({
            "normal": "object",
            "no_expressions": true
        }));
        assert!(pure_literal.is_literal());

        // Test that literal creates literals (even with $from-looking content)
        let from_like = ValueTemplate::literal(serde_json::json!({
            "$from": {"workflow": "input"},
            "path": "name"
        }));
        assert!(from_like.is_literal()); // literal doesn't parse expressions, creates literals
    }

    #[test]
    fn test_parse_vs_literal() {
        let json_with_expr = serde_json::json!({"$from": {"step": "step1"}});

        // parse() interprets expressions
        let parsed = ValueTemplate::parse_value(json_with_expr.clone()).unwrap();
        assert!(!parsed.is_literal()); // Contains an expression

        // literal() treats everything as literal data
        let literal = ValueTemplate::literal(json_with_expr.clone());
        assert!(literal.is_literal()); // Treats $from as literal JSON key

        // They produce different results
        assert_ne!(parsed, literal);

        // Demonstrate the improvement for complex objects:
        let complex_json = serde_json::json!({
            "data": {"$from": {"step": "step1"}},
            "config": {"$from": {"workflow": "input"}, "path": "config"},
            "literal_value": 42
        });

        // BEFORE (verbose, error-prone):
        // let old_way = serde_json::from_value(complex_json.clone()).unwrap();

        // AFTER (clear intent):
        let new_way = ValueTemplate::parse_value(complex_json.clone()).unwrap();

        // Both produce the same result
        let old_way: ValueTemplate = serde_json::from_value(complex_json).unwrap();
        assert_eq!(old_way, new_way);
    }

    #[test]
    fn test_literal_vs_parse() {
        let json_with_expr = serde_json::json!({"$from": {"step": "step1"}});

        // literal() treats JSON as literal data (never fails)
        let literal_result = ValueTemplate::literal(json_with_expr.clone());
        assert!(literal_result.is_literal()); // Treats $from as literal JSON key

        // parse() parses expressions (can fail)
        let parse_result = ValueTemplate::parse_value(json_with_expr.clone());
        assert!(parse_result.is_ok());
        let template = parse_result.unwrap();
        assert!(!template.is_literal()); // Parsed as expression

        // They produce different results
        assert_ne!(literal_result, template);

        // Test that parse() can fail with malformed JSON string
        let malformed_json = r#"{"$from": {"step": "step1""#; // Missing closing braces
        let invalid_result = serde_json::from_str::<serde_json::Value>(malformed_json)
            .and_then(ValueTemplate::parse_value);
        assert!(invalid_result.is_err()); // Should fail to parse malformed JSON

        // But literal() always succeeds by treating any valid JSON as literal data
        let some_json = serde_json::json!({"$from": "anything"});
        let literal_result = ValueTemplate::literal(some_json);
        assert!(literal_result.is_literal()); // Treats as literal data
    }

    #[test]
    fn test_schema_generation() {
        use schemars::schema_for;

        // Generate schema for ValueTemplate to see what it looks like
        let schema = schema_for!(ValueTemplate);
        let json_schema = serde_json::to_value(&schema).unwrap();

        // Basic check that it includes both expressions and values
        let schema_str = serde_json::to_string(&json_schema).unwrap();
        assert!(schema_str.contains("$from") || schema_str.contains("Expr"));
        // Should also support $literal now
        assert!(schema_str.contains("$literal") || schema_str.contains("LiteralValue"));
    }

    #[test]
    fn test_expression_iterator() {
        // Test simple expression
        let simple_expr = ValueTemplate::step_ref("step1", JsonPath::default());
        let expressions: Vec<_> = simple_expr.expressions().collect();
        assert_eq!(expressions.len(), 1);

        // Test object with multiple expressions
        let mut obj = IndexMap::new();
        obj.insert(
            "step1_ref".to_string(),
            ValueTemplate::step_ref("step1", JsonPath::default()),
        );
        obj.insert(
            "input_ref".to_string(),
            ValueTemplate::workflow_input(JsonPath::from("field")),
        );
        obj.insert(
            "literal".to_string(),
            ValueTemplate::string("literal_value"),
        );
        let template = ValueTemplate::object(obj);

        let expressions: Vec<_> = template.expressions().collect();
        assert_eq!(expressions.len(), 2); // Only expressions, not literals

        // Test nested structure
        let mut nested_obj = IndexMap::new();
        nested_obj.insert(
            "inner_expr".to_string(),
            ValueTemplate::step_ref("step2", JsonPath::default()),
        );

        let mut outer_obj = IndexMap::new();
        outer_obj.insert("nested".to_string(), ValueTemplate::object(nested_obj));
        outer_obj.insert(
            "top_level".to_string(),
            ValueTemplate::workflow_input(JsonPath::default()),
        );

        let complex_template = ValueTemplate::object(outer_obj);
        let expressions: Vec<_> = complex_template.expressions().collect();
        assert_eq!(expressions.len(), 2); // Both nested and top-level expressions

        // Test array with expressions
        let array_template = ValueTemplate::array(vec![
            ValueTemplate::step_ref("step1", JsonPath::default()),
            ValueTemplate::string("literal"),
            ValueTemplate::workflow_input(JsonPath::from("field")),
        ]);
        let expressions: Vec<_> = array_template.expressions().collect();
        assert_eq!(expressions.len(), 2); // Only expressions, not literal string

        // Test empty/literal values
        let literal_template = ValueTemplate::literal(serde_json::json!({"key": "value"}));
        let expressions: Vec<_> = literal_template.expressions().collect();
        assert_eq!(expressions.len(), 0); // No expressions in literal template
    }

    #[test]
    fn test_invalid_from_syntax() {
        // Test that invalid $from syntax fails to parse
        let invalid_from_string = r#"{"$from": "input"}"#;
        let result = serde_json::from_str::<ValueTemplate>(invalid_from_string);
        assert!(result.is_err(), "Should fail to parse invalid $from syntax");

        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("expected a map for $from"),
            "Error should mention invalid $from: {error_message}"
        );

        // Test that invalid $from syntax in nested objects also fails
        let invalid_nested = r#"{"data": {"$from": "input"}, "other": "value"}"#;
        let result = serde_json::from_str::<ValueTemplate>(invalid_nested);
        assert!(
            result.is_err(),
            "Should fail to parse invalid $from syntax in nested objects"
        );

        // Test that invalid $from syntax in arrays also fails
        let invalid_array = r#"[{"$from": "input"}, "literal_value"]"#;
        let result = serde_json::from_str::<ValueTemplate>(invalid_array);
        assert!(
            result.is_err(),
            "Should fail to parse invalid $from syntax in arrays"
        );
    }

    #[test]
    fn test_valid_from_syntax() {
        // Test that valid $from syntax parses correctly
        let valid_step_ref = r#"{"$from": {"step": "step1"}}"#;
        let result = serde_json::from_str::<ValueTemplate>(valid_step_ref);
        assert!(result.is_ok(), "Should parse valid step reference");
        assert!(!result.unwrap().is_literal(), "Should be an expression");

        let valid_workflow_ref = r#"{"$from": {"workflow": "input"}}"#;
        let result = serde_json::from_str::<ValueTemplate>(valid_workflow_ref);
        assert!(result.is_ok(), "Should parse valid workflow reference");
        assert!(!result.unwrap().is_literal(), "Should be an expression");

        let valid_with_path = r#"{"$from": {"step": "step1"}, "path": "field"}"#;
        let result = serde_json::from_str::<ValueTemplate>(valid_with_path);
        assert!(result.is_ok(), "Should parse valid reference with path");
        assert!(!result.unwrap().is_literal(), "Should be an expression");
    }

    #[test]
    fn test_expression_iterator_respects_escaped_literals() {
        // Test that the expression iterator doesn't traverse into $literal blocks
        // even when they contain objects with nested expressions
        let template_json = r#"{
            "regular_field": {"$from": {"step": "step1"}},
            "literal_field": {
                "$literal": {
                    "nested_object": {
                        "inner_ref": {"$from": {"step": "step2"}},
                        "deep": {
                            "another_ref": {"$from": {"workflow": "input"}}
                        }
                    }
                }
            },
            "another_regular": {"$from": {"workflow": "input"}}
        }"#;

        let template: ValueTemplate = serde_json::from_str(template_json).unwrap();
        let expressions: Vec<_> = template.expressions().collect();

        // Should only find 3 expressions:
        // 1. {"$from": {"step": "step1"}} in regular_field
        // 2. The $literal expression itself (but not its content)
        // 3. {"$from": {"workflow": "input"}} in another_regular
        //
        // Importantly, the expressions INSIDE the $literal block should NOT be found:
        // - {"$from": {"step": "step2"}} should not appear
        // - {"$from": {"workflow": "input"}} inside the literal should not appear
        assert_eq!(expressions.len(), 3, "Should find exactly 3 expressions");

        // Check that we have the expected expression types
        let mut escaped_literal_count = 0;
        let mut regular_ref_count = 0;

        for expr in &expressions {
            match expr {
                crate::workflow::Expr::EscapedLiteral { .. } => {
                    escaped_literal_count += 1;
                }
                crate::workflow::Expr::Ref { .. } => {
                    regular_ref_count += 1;
                }
                _ => panic!("Unexpected expression type: {expr:?}"),
            }
        }

        assert_eq!(
            escaped_literal_count, 1,
            "Should find exactly 1 escaped literal"
        );
        assert_eq!(
            regular_ref_count, 2,
            "Should find exactly 2 regular references"
        );

        // Verify that step "step2" is NOT found as a separate expression
        // (it's inside the $literal block and should be treated as opaque data)
        let step_refs: Vec<_> = expressions
            .iter()
            .filter_map(|expr| match expr {
                crate::workflow::Expr::Ref {
                    from: crate::workflow::BaseRef::Step { step },
                    ..
                } => Some(step.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(step_refs.len(), 1, "Should find exactly 1 step reference");
        assert_eq!(
            step_refs[0], "step1",
            "Should only find reference to step1, not step2 from inside $literal"
        );
    }
}
