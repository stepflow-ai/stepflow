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

use schemars::JsonSchema;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A single part of a JSON path.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum PathPart {
    /// Access a field by name
    Field(String),
    /// Access an array element by index
    Index(usize),
    /// Access an element by string key (for arrays that can be indexed by string)
    IndexStr(String),
}

/// A JSON path represented as a sequence of path parts.
/// This type serializes and deserializes as a string but internally
/// maintains a structured representation for efficient processing.
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub struct JsonPath(Vec<PathPart>);

impl JsonPath {
    /// Create a new empty path
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a path from a vector of parts
    pub fn from_parts(parts: Vec<PathPart>) -> Self {
        Self(parts)
    }

    /// Get the parts of the path
    pub fn parts(&self) -> &[PathPart] {
        &self.0
    }

    /// Check if the path is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn outer_field(&self) -> Option<&str> {
        match self.0.first() {
            Some(PathPart::Field(name)) => Some(name),
            Some(PathPart::IndexStr(name)) => Some(name),
            _ => None,
        }
    }

    /// Parse a string path into a JsonPath
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Ok(Self::new());
        }

        let s = match s.trim().strip_prefix("$") {
            Some(s) => s,
            None => {
                // If it doesn't start with $, treat it as a bare field name
                return Ok(Self::from_parts(vec![PathPart::Field(s.to_string())]));
            }
        };

        let mut parts = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '[' => {
                    let mut bracket_content = String::new();
                    let mut found_end = false;

                    for ch in chars.by_ref() {
                        if ch == ']' {
                            found_end = true;
                            break;
                        }
                        bracket_content.push(ch);
                    }

                    if !found_end {
                        return Err("Unclosed bracket '[' in path".to_string());
                    }

                    let bracket_content = bracket_content.trim();

                    // Check if it's a quoted string
                    if (bracket_content.starts_with('"') && bracket_content.ends_with('"'))
                        || (bracket_content.starts_with('\'') && bracket_content.ends_with('\''))
                    {
                        let field_name = &bracket_content[1..bracket_content.len() - 1];
                        parts.push(PathPart::Field(field_name.to_string()));
                    } else if let Ok(index) = bracket_content.parse::<usize>() {
                        parts.push(PathPart::Index(index));
                    } else {
                        return Err(format!(
                            "Invalid index '{bracket_content}' in JSON path. Expected a number or a quoted string."
                        ));
                    }
                }
                '.' => {
                    let mut field_name = String::new();

                    while let Some(&ch) = chars.peek() {
                        if ch == '[' || ch == '.' {
                            break;
                        }
                        field_name.push(chars.next().unwrap());
                    }

                    if !field_name.is_empty() {
                        parts.push(PathPart::Field(field_name));
                    }
                }
                _ => {
                    return Err(format!(
                        "Invalid character '{ch}' in JSON path after $ or ']'. Expected '.' or '['"
                    ));
                }
            }
        }

        Ok(Self::from_parts(parts))
    }

    /// Convert the path back to string representation
    fn as_string(&self) -> String {
        let mut result = String::from("$");

        for part in &self.0 {
            match part {
                PathPart::Field(name) => {
                    result.push_str(&format!(".{name}"));
                }
                PathPart::Index(idx) => {
                    result.push_str(&format!("[{idx}]"));
                }
                PathPart::IndexStr(s) => {
                    result.push_str(&format!("[\"{s}\"]"));
                }
            }
        }

        result
    }
}

impl Default for JsonPath {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JsonPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

impl Serialize for JsonPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct JsonPathVisitor;

        impl<'de> Visitor<'de> for JsonPathVisitor {
            type Value = JsonPath;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON path string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                JsonPath::parse(value).map_err(de::Error::custom)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(JsonPath::new())
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_str(self)
            }
        }

        deserializer.deserialize_option(JsonPathVisitor)
    }
}

impl JsonSchema for JsonPath {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "JsonPath".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "JSON path expression to apply to the referenced value. May use `$` to reference the whole value. May also be a bare field name (without the leading $) if the referenced value is an object.",
            "examples": ["field", "$.field", "$[\"field\"]", "$[0]", "$.field[0].nested"]
        })
    }
}

impl utoipa::PartialSchema for JsonPath {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        // Use a simple string schema
        utoipa::openapi::RefOr::T(utoipa::openapi::Schema::AllOf(
            utoipa::openapi::AllOfBuilder::new()
                .description(Some("JSON path expression to apply to the referenced value. May use `$` to reference the whole value. May also be a bare field name (without the leading $) if the referenced value is an object."))
                .build()
        ))
    }
}

impl utoipa::ToSchema for JsonPath {}

impl From<String> for JsonPath {
    fn from(s: String) -> Self {
        Self::parse(&s).unwrap_or_default()
    }
}

impl From<&str> for JsonPath {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_path_parsing() {
        // Test backwards compatibility - bare field name
        let path = JsonPath::parse("field").unwrap();
        assert_eq!(path.parts(), &[PathPart::Field("field".to_string())]);
        assert_eq!(path.to_string(), "$.field");

        // Test $ prefix field access
        let path = JsonPath::parse("$[\"field\"]").unwrap();
        assert_eq!(path.parts(), &[PathPart::Field("field".to_string())]);
        assert_eq!(path.to_string(), "$.field");

        // Test dot notation
        let path = JsonPath::parse("$.field").unwrap();
        assert_eq!(path.parts(), &[PathPart::Field("field".to_string())]);
        assert_eq!(path.to_string(), "$.field");

        // Test array index
        let path = JsonPath::parse("$[0]").unwrap();
        assert_eq!(path.parts(), &[PathPart::Index(0)]);
        assert_eq!(path.to_string(), "$[0]");

        // Test complex path
        let path = JsonPath::parse("$.field[0].nested").unwrap();
        assert_eq!(
            path.parts(),
            &[
                PathPart::Field("field".to_string()),
                PathPart::Index(0),
                PathPart::Field("nested".to_string())
            ]
        );
        assert_eq!(path.to_string(), "$.field[0].nested");

        // Test empty path
        let path = JsonPath::parse("").unwrap();
        assert!(path.is_empty());
        assert_eq!(path.to_string(), "$");
    }
    #[test]

    fn test_json_path_invalid_syntax() {
        // Invalid -- missing `.` or `[` after `$`
        assert_eq!(
            JsonPath::parse("$field").unwrap_err(),
            "Invalid character 'f' in JSON path after $ or ']'. Expected '.' or '['"
        );

        // Missing `.` or `[` after `]`
        assert_eq!(
            JsonPath::parse("$[0]field").unwrap_err(),
            "Invalid character 'f' in JSON path after $ or ']'. Expected '.' or '['"
        );

        // Unterminated string
        assert_eq!(
            JsonPath::parse("$[\"hello").unwrap_err(),
            "Unclosed bracket '[' in path"
        );
        // Unterminated square-brackets (string key)
        assert_eq!(
            JsonPath::parse("$[\"hello\"").unwrap_err(),
            "Unclosed bracket '[' in path"
        );
        // Unterminated square-brackets (numeric key)
        assert_eq!(
            JsonPath::parse("$[5").unwrap_err(),
            "Unclosed bracket '[' in path"
        );
        // Invalid non-digit in square brackets
        assert_eq!(
            JsonPath::parse("$[hello]").unwrap_err(),
            "Invalid index 'hello' in JSON path. Expected a number or a quoted string."
        );
    }

    #[test]
    fn test_json_path_serialization() {
        // Test that single field serializes with $ notation (changed from backwards compatibility)
        let path = JsonPath::from("field");
        let serialized = serde_json::to_string(&path).unwrap();
        assert_eq!(serialized, "\"$.field\"");

        // Test that complex path serializes with $ notation
        let path = JsonPath::parse("$.field[0]").unwrap();
        let serialized = serde_json::to_string(&path).unwrap();
        assert_eq!(serialized, "\"$.field[0]\"");

        // Test round-trip
        let original = JsonPath::parse("$.items[0].name").unwrap();
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: JsonPath = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_json_path_deserialization() {
        let deserialized: JsonPath = serde_json::from_str(&"\"$\"").unwrap();
        assert_eq!(deserialized, JsonPath::parse("$").unwrap());

        let deserialized: JsonPath = serde_json::from_str(&"null").unwrap();
        assert_eq!(deserialized, JsonPath::new());
    }
}
