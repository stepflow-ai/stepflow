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

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Identifies a specific plugin and atomic functionality to execute.
///
/// A component is identified by a path that specifies:
/// - The plugin name
/// - The component name within that plugin
/// - Optional sub-path for specific functionality
///
/// Components can be specified as:
/// - Full paths: "/mock/test", "/mcp/my-tool/component"
/// - Builtin names: "eval", "load_file" (treated as builtin components)
#[derive(Debug, Eq, PartialEq, Clone, Hash, utoipa::ToSchema)]
pub struct Component {
    /// The component path as a string
    path: String,
    /// Position of the first slash in the path, if any (for efficient parsing)
    delimiter: usize,
}

impl JsonSchema for Component {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Component".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf')."
        })
    }
}

impl PartialOrd for Component {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Component {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl Component {
    /// Creates a new component for a specific plugin.
    pub fn for_plugin(plugin: &str, component_path: &str) -> Self {
        Self {
            path: format!("/{plugin}/{component_path}"),
            delimiter: 1, // Position after the first slash
        }
    }

    /// Creates a component from a string, handling both paths and builtin names.
    pub fn from_string(input: &str) -> Self {
        if input.starts_with('/') {
            // Path-based component
            match input[1..].find('/') {
                Some(pos) => Self {
                    path: input.to_string(),
                    delimiter: pos + 2, // Position after the plugin name and second slash
                },
                None => {
                    // Just plugin name like "/plugin", treat as error or default
                    Self {
                        path: input.to_string(),
                        delimiter: input.len(),
                    }
                }
            }
        } else {
            // Builtin component (no leading slash)
            Self {
                path: input.to_string(),
                delimiter: 0, // No plugin delimiter for builtins
            }
        }
    }

    /// Returns the full path string of the component.
    pub fn path_string(&self) -> &str {
        &self.path
    }

    /// Returns true if this is a builtin component.
    pub fn is_builtin(&self) -> bool {
        !self.path.starts_with('/')
    }

    /// Returns the builtin name if this is a builtin component.
    pub fn builtin_name(&self) -> Option<&str> {
        if self.is_builtin() {
            Some(&self.path)
        } else {
            None
        }
    }

    /// Returns the plugin name of the component.
    ///
    /// For example, for "/mcp-files/example", returns "mcp-files".
    /// For builtin components, returns "builtin".
    pub fn plugin(&self) -> &str {
        if self.is_builtin() {
            "builtin"
        } else {
            // Find the plugin name between the first and second slash
            let without_leading_slash = &self.path[1..];
            match without_leading_slash.find('/') {
                Some(pos) => &without_leading_slash[0..pos],
                None => without_leading_slash, // Only plugin name, no component path
            }
        }
    }
}

// Custom serialization: output just the path string
impl Serialize for Component {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // For builtin components, serialize just the builtin name
        if self.is_builtin() {
            self.builtin_name().unwrap_or("").serialize(serializer)
        } else {
            self.path.serialize(serializer)
        }
    }
}

// Custom deserialization: parse delimiter and handle builtins
impl<'de> Deserialize<'de> for Component {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Ok(Self::from_string(&input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_new() {
        let component = Component::for_plugin("mock", "test");
        assert_eq!(component.path_string(), "/mock/test");
        assert_eq!(component.plugin(), "mock");
        assert!(!component.is_builtin());
    }

    #[test]
    fn test_component_builtin() {
        let component = Component::from_string("eval");
        assert_eq!(component.path_string(), "eval");
        assert_eq!(component.plugin(), "builtin");
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some("eval"));
    }

    #[test]
    fn test_component_from_string_path() {
        let component = Component::from_string("/mcp/tool/component");
        assert_eq!(component.path_string(), "/mcp/tool/component");
        assert_eq!(component.plugin(), "mcp");
        assert!(!component.is_builtin());
    }

    #[test]
    fn test_component_from_string_builtin() {
        let component = Component::from_string("load_file");
        assert_eq!(component.plugin(), "builtin");
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some("load_file"));
    }

    #[test]
    fn test_component_serialization() {
        // Test builtin serialization
        let component = Component::from_string("eval");
        let json = serde_json::to_string(&component).unwrap();
        assert_eq!(json, "\"eval\"");

        // Test path serialization
        let component = Component::from_string("/mock/test");
        let json = serde_json::to_string(&component).unwrap();
        assert_eq!(json, "\"/mock/test\"");
    }

    #[test]
    fn test_component_deserialization() {
        // Test builtin deserialization
        let component: Component = serde_json::from_str("\"eval\"").unwrap();
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some("eval"));

        // Test path deserialization
        let component: Component = serde_json::from_str("\"/mock/test\"").unwrap();
        assert!(!component.is_builtin());
        assert_eq!(component.path_string(), "/mock/test");
    }

    #[test]
    fn test_component_roundtrip() {
        let test_cases = vec!["eval", "load_file", "/mock/test", "/mcp/tool/comp"];

        for case in test_cases {
            let component = Component::from_string(case);
            let json = serde_json::to_string(&component).unwrap();
            let deserialized: Component = serde_json::from_str(&json).unwrap();

            if component.is_builtin() {
                assert_eq!(component.builtin_name(), deserialized.builtin_name());
            } else {
                assert_eq!(component.path_string(), deserialized.path_string());
            }
        }
    }

    #[test]
    fn test_new_plugin() {
        let component = Component::for_plugin("mcp", "tool/component");
        assert_eq!(component.path_string(), "/mcp/tool/component");
        assert_eq!(component.plugin(), "mcp");
        assert!(!component.is_builtin());
    }
}
