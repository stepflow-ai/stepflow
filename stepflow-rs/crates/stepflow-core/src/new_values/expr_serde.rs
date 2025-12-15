// Custom deserialization for ValueExpr
//
// We implement custom deserialization by first deserializing to serde_json::Value,
// then parsing that into ValueExpr. This avoids the trial-and-error overhead of
// untagged enums and gives us precise control over parsing.
//
// We could further speed this up by writing our own deserializer directly to ValueExpr
// but that would require more special handling.

use super::expr::ValueExpr;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

impl Serialize for ValueExpr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap as _;

        match self {
            ValueExpr::Step { step, path } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("$step", step)?;
                if !path.is_empty() {
                    map.serialize_entry("path", path)?;
                }
                map.end()
            }
            ValueExpr::Input { input } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$input", input)?;
                map.end()
            }
            ValueExpr::Variable { variable, default } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("$variable", variable)?;
                if let Some(def) = default {
                    map.serialize_entry("default", def)?;
                }
                map.end()
            }
            ValueExpr::EscapedLiteral { literal } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$literal", literal)?;
                map.end()
            }
            ValueExpr::If { condition, then, else_expr } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("$if", condition)?;
                map.serialize_entry("then", then)?;
                if let Some(else_val) = else_expr {
                    map.serialize_entry("else", else_val)?;
                }
                map.end()
            }
            ValueExpr::Coalesce { values } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$coalesce", values)?;
                map.end()
            }
            ValueExpr::Array(items) => items.serialize(serializer),
            ValueExpr::Object(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            ValueExpr::Literal(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ValueExpr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_value_expr(value).map_err(de::Error::custom)
    }
}

/// Parse a serde_json::Value into a ValueExpr
///
/// Detection order:
/// 1. Check for $ prefixed keys ($step, $input, $variable, $literal)
/// 2. Recursively parse arrays/objects
/// 3. Fall through to Literal for primitives
fn parse_value_expr(value: Value) -> Result<ValueExpr, String> {
    match value {
        Value::Object(obj) if obj.contains_key("$literal") => {
            // EscapedLiteral - extract the literal value
            let literal = obj
                .get("$literal")
                .ok_or_else(|| "$literal key not found".to_string())?
                .clone();
            Ok(ValueExpr::EscapedLiteral { literal })
        }
        Value::Object(ref obj) if obj.contains_key("$if") => {
            // Conditional expression
            let condition = obj
                .get("$if")
                .ok_or_else(|| "$if key not found".to_string())?;
            let then = obj
                .get("then")
                .ok_or_else(|| "then key required for $if".to_string())?;
            let else_expr = obj.get("else");

            let condition_expr = parse_value_expr(condition.clone())?;
            let then_expr = parse_value_expr(then.clone())?;
            let else_opt = else_expr
                .map(|e| parse_value_expr(e.clone()))
                .transpose()?;

            Ok(ValueExpr::If {
                condition: Box::new(condition_expr),
                then: Box::new(then_expr),
                else_expr: else_opt.map(Box::new),
            })
        }
        Value::Object(ref obj) if obj.contains_key("$coalesce") => {
            // Coalesce expression
            let values_val = obj
                .get("$coalesce")
                .ok_or_else(|| "$coalesce key not found".to_string())?;

            let values_array = values_val
                .as_array()
                .ok_or_else(|| "$coalesce must be an array".to_string())?;

            let mut values = Vec::new();
            for v in values_array {
                values.push(parse_value_expr(v.clone())?);
            }

            Ok(ValueExpr::Coalesce { values })
        }
        Value::Object(ref obj) if obj.contains_key("$step") => {
            // Step reference - extract step and optional path
            let step = obj
                .get("$step")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "$step must be a string".to_string())?
                .to_string();

            let path = if let Some(path_value) = obj.get("path") {
                serde_json::from_value(path_value.clone())
                    .map_err(|e| format!("Invalid path: {}", e))?
            } else {
                super::JsonPath::default()
            };

            Ok(ValueExpr::Step { step, path })
        }
        Value::Object(ref obj) if obj.contains_key("$input") => {
            // Workflow input - extract input path
            let input = obj
                .get("$input")
                .ok_or_else(|| "$input key not found".to_string())?;

            let input_path: super::JsonPath = serde_json::from_value(input.clone())
                .map_err(|e| format!("Invalid $input: {}", e))?;

            Ok(ValueExpr::Input { input: input_path })
        }
        Value::Object(ref obj) if obj.contains_key("$variable") => {
            // Variable reference - extract variable name/path and optional default
            let variable = obj
                .get("$variable")
                .ok_or_else(|| "$variable key not found".to_string())?;

            let variable_path: super::JsonPath = serde_json::from_value(variable.clone())
                .map_err(|e| format!("Invalid $variable: {}", e))?;

            let default = if let Some(default_value) = obj.get("default") {
                let default_expr = parse_value_expr(default_value.clone())?;
                Some(Box::new(default_expr))
            } else {
                None
            };

            Ok(ValueExpr::Variable {
                variable: variable_path,
                default,
            })
        }
        Value::Object(obj) => {
            // Regular object - recurse on values
            let mut fields = Vec::new();
            for (k, v) in obj {
                let expr = parse_value_expr(v)?;
                fields.push((k, expr));
            }
            Ok(ValueExpr::Object(fields))
        }
        Value::Array(arr) => {
            // Array - recurse on elements
            let mut exprs = Vec::new();
            for v in arr {
                let expr = parse_value_expr(v)?;
                exprs.push(expr);
            }
            Ok(ValueExpr::Array(exprs))
        }
        // Primitives: null, bool, number, string
        primitive => Ok(ValueExpr::Literal(primitive)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_values::JsonPath;
    use serde_json::json;

    #[test]
    fn test_serde_step_reference() {
        let expr = ValueExpr::step("my_step", JsonPath::default());
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, r#"{"$step":"my_step"}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_step_with_path() {
        let expr = ValueExpr::step("my_step", JsonPath::from("result"));
        let json_str = serde_json::to_string(&expr).unwrap();
        // JsonPath normalizes "result" to "$.result"
        assert_eq!(json_str, r#"{"$step":"my_step","path":"$.result"}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);

        // Test that shorthand also parses correctly
        let shorthand: ValueExpr =
            serde_json::from_str(r#"{"$step":"my_step","path":"result"}"#).unwrap();
        assert_eq!(shorthand, expr);
    }

    #[test]
    fn test_serde_input() {
        let expr = ValueExpr::workflow_input(JsonPath::from("name"));
        let json_str = serde_json::to_string(&expr).unwrap();
        // JsonPath normalizes "name" to "$.name"
        assert_eq!(json_str, r#"{"$input":"$.name"}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);

        // Test that shorthand also parses correctly
        let shorthand: ValueExpr = serde_json::from_str(r#"{"$input":"name"}"#).unwrap();
        assert_eq!(shorthand, expr);
    }

    #[test]
    fn test_serde_input_root() {
        let expr = ValueExpr::workflow_input(JsonPath::from("$"));
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, r#"{"$input":"$"}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_variable() {
        let expr = ValueExpr::variable("api_key", None);
        let json_str = serde_json::to_string(&expr).unwrap();
        // JsonPath normalizes "api_key" to "$.api_key"
        assert_eq!(json_str, r#"{"$variable":"$.api_key"}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);

        // Test that shorthand also parses correctly
        let shorthand: ValueExpr = serde_json::from_str(r#"{"$variable":"api_key"}"#).unwrap();
        assert_eq!(shorthand, expr);
    }

    #[test]
    fn test_serde_variable_with_default() {
        let default = Box::new(ValueExpr::literal(json!("default_value")));
        let expr = ValueExpr::variable("my_var", Some(default));
        let json_str = serde_json::to_string(&expr).unwrap();
        // JsonPath normalizes "my_var" to "$.my_var"
        assert!(json_str.contains(r#""$variable":"$.my_var""#));
        assert!(json_str.contains(r#""default":"default_value""#));

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_escaped_literal() {
        let expr = ValueExpr::escaped_literal(json!({"step": "not_a_ref"}));
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, r#"{"$literal":{"step":"not_a_ref"}}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_primitives() {
        // Null
        let expr = ValueExpr::Literal(json!(null));
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, "null");
        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);

        // Bool
        let expr = ValueExpr::Literal(json!(true));
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, "true");
        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);

        // Number
        let expr = ValueExpr::Literal(json!(42));
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, "42");
        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);

        // String
        let expr = ValueExpr::Literal(json!("hello"));
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, r#""hello""#);
        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_array() {
        let expr = ValueExpr::Array(vec![
            ValueExpr::literal(json!(1)),
            ValueExpr::literal(json!("two")),
            ValueExpr::step("step1", JsonPath::default()),
        ]);
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, r#"[1,"two",{"$step":"step1"}]"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_object() {
        let expr = ValueExpr::Object(vec![
            ("a".to_string(), ValueExpr::literal(json!(1))),
            (
                "b".to_string(),
                ValueExpr::step("step1", JsonPath::default()),
            ),
        ]);
        let json_str = serde_json::to_string(&expr).unwrap();
        assert_eq!(json_str, r#"{"a":1,"b":{"$step":"step1"}}"#);

        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_serde_nested_structures() {
        // Complex nested structure
        let expr = ValueExpr::Object(vec![
            (
                "input".to_string(),
                ValueExpr::workflow_input(JsonPath::from("data")),
            ),
            (
                "steps".to_string(),
                ValueExpr::Array(vec![
                    ValueExpr::step("step1", JsonPath::from("result")),
                    ValueExpr::step("step2", JsonPath::default()),
                ]),
            ),
            (
                "config".to_string(),
                ValueExpr::Object(vec![
                    ("enabled".to_string(), ValueExpr::literal(json!(true))),
                    ("count".to_string(), ValueExpr::literal(json!(5))),
                ]),
            ),
        ]);

        let json_str = serde_json::to_string(&expr).unwrap();
        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn test_parse_error_invalid_step() {
        // Missing required field
        let json_str = r#"{"$step": 123}"#; // step must be string
        let result: Result<ValueExpr, _> = serde_json::from_str(json_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_object_without_dollar_keys() {
        // Regular object without $ keys should parse as Object variant
        let json_str = r#"{"a": 1, "b": "hello"}"#;
        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        match parsed {
            ValueExpr::Object(fields) => {
                assert_eq!(fields.len(), 2);
                assert!(
                    fields
                        .iter()
                        .any(|(k, v)| k == "a" && *v == ValueExpr::Literal(json!(1)))
                );
                assert!(
                    fields
                        .iter()
                        .any(|(k, v)| k == "b" && *v == ValueExpr::Literal(json!("hello")))
                );
            }
            _ => panic!("Expected Object variant, got {:?}", parsed),
        }
    }

    #[test]
    fn test_deep_nesting() {
        // Test deeply nested structures with mixed references and literals
        let json_str = r#"{
            "level1": {
                "level2": {
                    "level3": {
                        "ref": {"$step": "deep_step"},
                        "literal": "value",
                        "array": [
                            1,
                            {"$input": "nested"},
                            {
                                "level4": {
                                    "level5": {"$variable": "deep_var"}
                                }
                            }
                        ]
                    }
                }
            }
        }"#;

        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        // Verify it round-trips correctly
        let serialized = serde_json::to_string(&parsed).unwrap();
        let reparsed: ValueExpr = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, reparsed);

        // Verify structure
        match parsed {
            ValueExpr::Object(fields) => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0, "level1");

                match &fields[0].1 {
                    ValueExpr::Object(level2_fields) => {
                        assert_eq!(level2_fields.len(), 1);
                        assert_eq!(level2_fields[0].0, "level2");
                        // We have deep nesting
                    }
                    _ => panic!("Expected Object at level2"),
                }
            }
            _ => panic!("Expected Object at top level"),
        }
    }

    #[test]
    fn test_escaped_literal_with_dollar_keys() {
        // $literal should prevent parsing of nested $ keys
        let json_str = r#"{"$literal": {"$step": "not_a_ref", "$input": "also_not_a_ref"}}"#;
        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        match &parsed {
            ValueExpr::EscapedLiteral { literal } => {
                // The literal should contain the raw object
                assert_eq!(*literal, json!({"$step": "not_a_ref", "$input": "also_not_a_ref"}));
            }
            _ => panic!("Expected EscapedLiteral, got {:?}", parsed),
        }

        // Verify round-trip
        let serialized = serde_json::to_string(&parsed).unwrap();
        let reparsed: ValueExpr = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn test_escaped_literal_with_nested_literal() {
        // $literal containing another $literal structure (but as raw data)
        let json_str = r#"{"$literal": {"$literal": "inner"}}"#;
        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        match &parsed {
            ValueExpr::EscapedLiteral { literal } => {
                // The outer $literal escapes everything, so inner $literal is just data
                assert_eq!(*literal, json!({"$literal": "inner"}));
            }
            _ => panic!("Expected EscapedLiteral, got {:?}", parsed),
        }
    }

    #[test]
    fn test_literal_containing_dollar_prefixed_fields() {
        // Regular object (not $literal) with fields that happen to start with $
        // but aren't our special keys
        let json_str = r#"{"$custom": "value", "$other": 123, "normal": true}"#;
        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        match parsed {
            ValueExpr::Object(fields) => {
                assert_eq!(fields.len(), 3);
                // These should be treated as regular fields since they're not $step, $input, $variable, or $literal
                assert!(fields.iter().any(|(k, v)| k == "$custom" && *v == ValueExpr::Literal(json!("value"))));
                assert!(fields.iter().any(|(k, v)| k == "$other" && *v == ValueExpr::Literal(json!(123))));
                assert!(fields.iter().any(|(k, v)| k == "normal" && *v == ValueExpr::Literal(json!(true))));
            }
            _ => panic!("Expected Object, got {:?}", parsed),
        }
    }

    #[test]
    fn test_array_of_escaped_literals() {
        // Array containing multiple escaped literals
        let json_str = r#"[
            {"$literal": {"$step": "fake1"}},
            {"$literal": {"$input": "fake2"}},
            "normal_string"
        ]"#;
        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        match parsed {
            ValueExpr::Array(items) => {
                assert_eq!(items.len(), 3);

                // First item should be EscapedLiteral
                match &items[0] {
                    ValueExpr::EscapedLiteral { literal } => {
                        assert_eq!(*literal, json!({"$step": "fake1"}));
                    }
                    _ => panic!("Expected EscapedLiteral at index 0"),
                }

                // Second item should be EscapedLiteral
                match &items[1] {
                    ValueExpr::EscapedLiteral { literal } => {
                        assert_eq!(*literal, json!({"$input": "fake2"}));
                    }
                    _ => panic!("Expected EscapedLiteral at index 1"),
                }

                // Third item should be regular Literal
                assert_eq!(items[2], ValueExpr::Literal(json!("normal_string")));
            }
            _ => panic!("Expected Array, got {:?}", parsed),
        }
    }

    #[test]
    fn test_complex_mixed_nesting() {
        // Complex structure mixing all expression types
        let json_str = r#"{
            "steps_array": [
                {"$step": "step1"},
                {"$step": "step2", "path": "result"}
            ],
            "inputs": {
                "user_input": {"$input": "name"},
                "workflow_root": {"$input": "$"}
            },
            "config": {
                "api_key": {"$variable": "api_key", "default": "default_key"},
                "nested_config": {
                    "escaped": {"$literal": {"$step": "not_real"}},
                    "real_ref": {"$step": "real_step"},
                    "values": [1, 2, {"$variable": "count"}]
                }
            }
        }"#;

        let parsed: ValueExpr = serde_json::from_str(json_str).unwrap();

        // Verify round-trip
        let serialized = serde_json::to_string(&parsed).unwrap();
        let reparsed: ValueExpr = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, reparsed);

        // Verify top-level structure
        match parsed {
            ValueExpr::Object(fields) => {
                assert_eq!(fields.len(), 3);
                assert!(fields.iter().any(|(k, _)| k == "steps_array"));
                assert!(fields.iter().any(|(k, _)| k == "inputs"));
                assert!(fields.iter().any(|(k, _)| k == "config"));
            }
            _ => panic!("Expected Object at top level"),
        }
    }

    #[test]
    fn test_escaped_literal_with_complex_value() {
        // $literal with complex nested structure
        let complex_value = json!({
            "nested": {
                "arrays": [1, 2, 3],
                "objects": {"a": "b"},
                "fake_refs": {
                    "$step": "not_expanded",
                    "$input": "not_expanded",
                    "$variable": "not_expanded"
                }
            }
        });

        let expr = ValueExpr::escaped_literal(complex_value.clone());
        let json_str = serde_json::to_string(&expr).unwrap();
        let parsed: ValueExpr = serde_json::from_str(&json_str).unwrap();

        match &parsed {
            ValueExpr::EscapedLiteral { literal } => {
                assert_eq!(*literal, complex_value);
            }
            _ => panic!("Expected EscapedLiteral"),
        }
    }
}
