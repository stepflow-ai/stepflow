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

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PathPart {
    String(Cow<'static, str>),
    Index(usize),
}

impl From<&'static str> for PathPart {
    fn from(s: &'static str) -> Self {
        PathPart::String(Cow::Borrowed(s))
    }
}

impl From<String> for PathPart {
    fn from(value: String) -> Self {
        PathPart::String(Cow::Owned(value))
    }
}

impl From<usize> for PathPart {
    fn from(value: usize) -> Self {
        PathPart::Index(value)
    }
}

/// A path to a location in a workflow definition, used for diagnostic messages.
/// Serializes as a string like `$.steps[0].input`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Path(pub(crate) Vec<PathPart>);

fn needs_quotation(s: &str) -> bool {
    s.contains('.') || s.contains('[') || s.contains(']')
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.0.is_empty() {
            write!(f, "$")?;
            for part in self.0.iter() {
                match part {
                    PathPart::String(v) if needs_quotation(v) => write!(f, "[{v:?}]")?,
                    PathPart::String(v) => write!(f, ".{v}")?,
                    PathPart::Index(v) => write!(f, "[{v}]")?,
                }
            }
        }
        Ok(())
    }
}

// Custom serialization: serialize as a string
impl Serialize for Path {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

// Custom deserialization: parse from a string
impl<'de> Deserialize<'de> for Path {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PathVisitor;

        impl Visitor<'_> for PathVisitor {
            type Value = Path;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a path string like $.steps[0].input")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Path::parse(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(PathVisitor)
    }
}

// Custom OpenAPI schema: declare as a string type
impl utoipa::PartialSchema for Path {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::RefOr::T(utoipa::openapi::Schema::Object(
            utoipa::openapi::ObjectBuilder::new()
                .schema_type(utoipa::openapi::schema::SchemaType::Type(
                    utoipa::openapi::schema::Type::String,
                ))
                .description(Some(
                    "Path to a location in the workflow definition, serialized as a string",
                ))
                .examples(["$.steps[0].input", "$.output", "$.steps[2].component"])
                .build(),
        ))
    }
}

impl utoipa::ToSchema for Path {}

macro_rules! make_path {
    ($($es:expr),*) => {
        {
            $crate::validation::Path(vec![
                $($crate::validation::PathPart::from($es),)*
            ])
        }
    }
}

pub(crate) use make_path;

impl Path {
    pub fn new() -> Path {
        Path(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn push(&mut self, part: impl Into<PathPart>) {
        self.0.push(part.into());
    }

    pub(crate) fn pop(&mut self) {
        self.0.pop();
    }

    pub(crate) fn prepend(&mut self, prefix: &Path) {
        let mut new_path = prefix.0.clone();
        new_path.append(&mut self.0);
        self.0 = new_path;
    }

    /// Parse a path string like `$.steps[0].input` into a Path.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Ok(Self::new());
        }

        let s = match s.strip_prefix('$') {
            Some(rest) => rest,
            None => return Err(format!("Path must start with '$', got: {s}")),
        };

        if s.is_empty() {
            return Ok(Self::new());
        }

        let mut parts = Vec::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '.' => {
                    // Parse field name
                    let mut name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '.' || c == '[' {
                            break;
                        }
                        name.push(chars.next().unwrap());
                    }
                    if !name.is_empty() {
                        parts.push(PathPart::String(Cow::Owned(name)));
                    }
                }
                '[' => {
                    let mut content = String::new();
                    let is_quoted = chars.peek() == Some(&'"');
                    if is_quoted {
                        chars.next(); // consume opening quote
                    }

                    let mut found_end = false;
                    for c in chars.by_ref() {
                        if is_quoted {
                            if c == '"' {
                                // Expect closing bracket next
                                if chars.next() != Some(']') {
                                    return Err("Expected ']' after closing quote".to_string());
                                }
                                found_end = true;
                                break;
                            }
                        } else if c == ']' {
                            found_end = true;
                            break;
                        }
                        content.push(c);
                    }

                    if !found_end {
                        return Err("Unclosed bracket '[' in path".to_string());
                    }

                    if is_quoted {
                        parts.push(PathPart::String(Cow::Owned(content)));
                    } else if let Ok(idx) = content.parse::<usize>() {
                        parts.push(PathPart::Index(idx));
                    } else {
                        return Err(format!("Invalid index '{content}' in path"));
                    }
                }
                _ => {
                    return Err(format!(
                        "Invalid character '{ch}' in path. Expected '.' or '['"
                    ));
                }
            }
        }

        Ok(Path(parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_path() {
        // Basic path creations
        assert_eq!(make_path!().to_string(), "");
        assert_eq!(make_path!("hello").to_string(), "$.hello");
        assert_eq!(make_path!("hello", 5usize).to_string(), "$.hello[5]");
        assert_eq!(
            make_path!("hello", 5usize, "world").to_string(),
            "$.hello[5].world"
        );
    }

    #[test]
    fn test_path_parse() {
        // Empty path
        assert_eq!(Path::parse("").unwrap(), Path::new());
        assert_eq!(Path::parse("$").unwrap(), Path::new());

        // Simple field access
        let path = Path::parse("$.hello").unwrap();
        assert_eq!(path.to_string(), "$.hello");

        // Index access
        let path = Path::parse("$.hello[5]").unwrap();
        assert_eq!(path.to_string(), "$.hello[5]");

        // Nested path
        let path = Path::parse("$.hello[5].world").unwrap();
        assert_eq!(path.to_string(), "$.hello[5].world");

        // Quoted field with special characters
        let path = Path::parse(r#"$["field.with.dots"]"#).unwrap();
        assert_eq!(path.to_string(), r#"$["field.with.dots"]"#);
    }

    #[test]
    fn test_path_roundtrip() {
        let original = make_path!("steps", 0usize, "input");
        let serialized = serde_json::to_string(&original).unwrap();
        assert_eq!(serialized, r#""$.steps[0].input""#);

        let deserialized: Path = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original.to_string(), deserialized.to_string());
    }

    #[test]
    fn test_empty_path_serialization() {
        let path = Path::new();
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, r#""""#);
    }
}
