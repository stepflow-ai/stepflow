use std::str::FromStr as _;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

/// Identifies a specific plugin and atomic functionality to execute.
///
/// A component is identified by a URL that specifies:
/// - The protocol (e.g., "langflow", "mcp")
/// - The transport (e.g., "http", "stdio")
/// - The path to the specific functionality
///
/// Components can be specified as:
/// - Full URLs: "mock://test", "mcp+stdio://my-tool/component"
/// - Builtin names: "eval", "load_file" (treated as builtin components)
#[derive(Debug, Eq, PartialEq, Clone, Hash, JsonSchema, utoipa::ToSchema)]
pub struct Component {
    /// The component URL as a string
    url: String,
    /// Position of the first colon in the URL, if any (for efficient parsing)
    delimiter: Option<usize>,
}

impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

pub type ComponentKey = (Option<String>, String);

impl Component {
    /// Creates a new component from a URL.
    pub fn new(url: Url) -> Self {
        let url_str = url.as_str();
        let delimiter = url_str.find(':');
        Self {
            url: url_str.to_string(),
            delimiter,
        }
    }

    /// Creates a component from a string, handling both URLs and builtin names.
    pub fn from_string(input: &str) -> Self {
        let delimiter = input.find(':');

        // If no colon found, treat as builtin
        if delimiter.is_none() {
            Self {
                url: format!("builtin://{input}"),
                delimiter: Some(7), // Position of the colon in "builtin:"
            }
        } else {
            Self {
                url: input.to_string(),
                delimiter,
            }
        }
    }

    /// Returns the full URL string of the component.
    pub fn url_string(&self) -> &str {
        &self.url
    }

    /// Parses the URL and returns a Url object if valid.
    pub fn url(&self) -> Result<Url, url::ParseError> {
        Url::from_str(&self.url)
    }

    /// Returns true if this is a builtin component.
    pub fn is_builtin(&self) -> bool {
        self.url.starts_with("builtin://")
    }

    /// Returns the builtin name if this is a builtin component.
    pub fn builtin_name(&self) -> Option<&str> {
        if self.is_builtin() {
            Some(&self.url[10..]) // Skip "builtin://"
        } else {
            None
        }
    }

    /// Returns the host of the component, if present and URL is valid.
    pub fn host(&self) -> Option<String> {
        self.url().ok()?.host_str().map(|s| s.to_string())
    }

    /// Returns the path component of the URL, if URL is valid.
    pub fn path(&self) -> Option<String> {
        Some(self.url().ok()?.path().to_string())
    }

    /// Returns the protocol and transport parts of the URL.
    ///
    /// For example, for "mcp+http://example.com", returns ("mcp", Some("http")).
    /// For builtin components, returns ("builtin", None).
    pub fn protocol_transport(&self) -> (&str, Option<&str>) {
        if let Some(delimiter_pos) = self.delimiter {
            let scheme = &self.url[..delimiter_pos];
            match scheme.split_once("+") {
                Some((protocol, transport)) => (protocol, Some(transport)),
                None => (scheme, None),
            }
        } else {
            // No colon found, shouldn't happen with our constructor
            ("", None)
        }
    }

    /// Returns the protocol part of the URL.
    ///
    /// For example, for "mcp+http://example.com", returns "mcp".
    /// For builtin components, returns "builtin".
    pub fn protocol(&self) -> &str {
        self.protocol_transport().0
    }

    /// Returns the transport part of the URL, if present.
    ///
    /// For example, for "mcp+http://example.com", returns Some("http").
    pub fn transport(&self) -> Option<&str> {
        self.protocol_transport().1
    }

    /// Parses a string into a Component URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid URL and not a valid builtin name.
    pub fn parse(input: &str) -> Result<Self, ComponentParseError> {
        let component = Self::from_string(input);

        // Validate the component
        if component.is_builtin() {
            // For builtins, just check that the name is not empty
            if component.builtin_name().unwrap_or("").trim().is_empty() {
                return Err(ComponentParseError::EmptyBuiltinName);
            }
        } else {
            // For URLs, validate that it's a proper URL
            component.url().map_err(ComponentParseError::InvalidUrl)?;
        }

        Ok(component)
    }

    /// Checks if this component URL is valid.
    pub fn is_valid(&self) -> bool {
        if self.is_builtin() {
            self.builtin_name()
                .is_some_and(|name| !name.trim().is_empty())
        } else {
            self.url().is_ok()
        }
    }

    pub fn key(&self) -> ComponentKey {
        if self.is_builtin() {
            (None, self.builtin_name().unwrap_or("").to_string())
        } else if let Ok(url) = self.url() {
            (
                url.host_str().map(|s| s.to_string()),
                url.path().to_string(),
            )
        } else {
            (None, String::new())
        }
    }
}

/// Errors that can occur when parsing a Component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentParseError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(url::ParseError),
    #[error("Empty builtin component name")]
    EmptyBuiltinName,
}

// Custom serialization: output just the URL string
impl Serialize for Component {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // For builtin components, serialize just the builtin name without "builtin://" prefix
        if self.is_builtin() {
            self.builtin_name().unwrap_or("").serialize(serializer)
        } else {
            self.url.serialize(serializer)
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
    fn test_component_from_url() {
        let url = Url::parse("mock://test").unwrap();
        let component = Component::new(url);
        assert_eq!(component.url_string(), "mock://test");
        assert_eq!(component.protocol(), "mock");
        assert_eq!(component.transport(), None);
        assert!(component.is_valid());
        assert!(!component.is_builtin());
    }

    #[test]
    fn test_component_builtin() {
        let component = Component::from_string("eval");
        assert_eq!(component.url_string(), "builtin://eval");
        assert_eq!(component.protocol(), "builtin");
        assert_eq!(component.transport(), None);
        assert!(component.is_valid());
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some("eval"));
    }

    #[test]
    fn test_component_parse_url() {
        let component = Component::parse("mcp+stdio://tool/component").unwrap();
        assert_eq!(component.url_string(), "mcp+stdio://tool/component");
        assert_eq!(component.protocol(), "mcp");
        assert_eq!(component.transport(), Some("stdio"));
        assert!(component.is_valid());
        assert!(!component.is_builtin());
    }

    #[test]
    fn test_component_parse_builtin() {
        let component = Component::parse("load_file").unwrap();
        assert_eq!(component.protocol(), "builtin");
        assert!(component.is_valid());
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some("load_file"));
    }

    #[test]
    fn test_component_invalid_url() {
        let component = Component::from_string("http://[invalid");
        assert!(!component.is_valid());
        assert!(!component.is_builtin());
    }

    #[test]
    fn test_component_empty_builtin() {
        let component = Component::from_string("");
        assert!(!component.is_valid());
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some(""));
    }

    #[test]
    fn test_component_serialization() {
        // Test builtin serialization
        let component = Component::from_string("eval");
        let json = serde_json::to_string(&component).unwrap();
        assert_eq!(json, "\"eval\"");

        // Test URL serialization
        let component = Component::from_string("mock://test");
        let json = serde_json::to_string(&component).unwrap();
        assert_eq!(json, "\"mock://test\"");
    }

    #[test]
    fn test_component_deserialization() {
        // Test builtin deserialization
        let component: Component = serde_json::from_str("\"eval\"").unwrap();
        assert!(component.is_builtin());
        assert_eq!(component.builtin_name(), Some("eval"));

        // Test URL deserialization
        let component: Component = serde_json::from_str("\"mock://test\"").unwrap();
        assert!(!component.is_builtin());
        assert_eq!(component.url_string(), "mock://test");
    }

    #[test]
    fn test_component_roundtrip() {
        let test_cases = vec!["eval", "load_file", "mock://test", "mcp+stdio://tool/comp"];

        for case in test_cases {
            let component = Component::from_string(case);
            let json = serde_json::to_string(&component).unwrap();
            let deserialized: Component = serde_json::from_str(&json).unwrap();

            if component.is_builtin() {
                assert_eq!(component.builtin_name(), deserialized.builtin_name());
            } else {
                assert_eq!(component.url_string(), deserialized.url_string());
            }
        }
    }
}
