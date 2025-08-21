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
use serde::{Deserialize, Serialize};

/// Identifies a specific plugin and atomic functionality to execute.
///
/// A component is identified by a path that specifies:
/// - The plugin name
/// - The component name within that plugin
/// - Optional sub-path for specific functionality
#[derive(
    Debug, Eq, PartialEq, Clone, Hash, utoipa::ToSchema, Serialize, Deserialize, Ord, PartialOrd,
)]
#[repr(transparent)]
pub struct Component(String);

impl JsonSchema for Component {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Component".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf').",
            "examples": [
                "/builtin/eval",
                "/mcpfs/list_files",
                "/python/udf",
            ]
        })
    }
}

impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Component {
    /// Creates a new component for a specific plugin.
    pub fn for_plugin(plugin: &str, component_path: &str) -> Self {
        debug_assert!(
            component_path.starts_with('/'),
            "component_path must start with '/'"
        );
        Self(format!("/{plugin}{component_path}"))
    }

    /// Creates a component from a string, handling both paths and builtin names.
    pub fn from_string(input: impl Into<String>) -> Self {
        Self(input.into())
    }

    /// Returns the full path string of the component.
    pub fn path(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_new() {
        let component = Component::for_plugin("mock", "/test");
        assert_eq!(component.path(), "/mock/test");
    }

    #[test]
    fn test_component_serialization() {
        let component = Component::from_string("/mock/test");
        let json = serde_json::to_string(&component).unwrap();
        assert_eq!(json, "\"/mock/test\"");
    }

    #[test]
    fn test_component_deserialization() {
        // Test path deserialization
        let component: Component = serde_json::from_str("\"/mock/test\"").unwrap();
        assert_eq!(component.path(), "/mock/test");
    }

    #[test]
    fn test_new_plugin() {
        let component = Component::for_plugin("mcp", "/tool/component");
        assert_eq!(component.path(), "/mcp/tool/component");
    }
}
