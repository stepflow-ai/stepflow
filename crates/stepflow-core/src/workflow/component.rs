use std::str::FromStr;

use schemars::JsonSchema;
use url::Url;

/// Identifies a specific plugin and atomic functionality to execute.
///
/// A component is identified by a URL that specifies:
/// - The protocol (e.g., "langflow", "mcp")
/// - The transport (e.g., "http", "stdio")
/// - The path to the specific functionality
#[derive(Debug, Eq, PartialEq, Clone, Hash, serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Component {
    url: Url,
}

impl std::fmt::Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

pub type ComponentKey<'a> = (Option<&'a str>, &'a str);

impl Component {
    /// Creates a new component from a URL.
    pub fn new(url: Url) -> Self {
        Self { url }
    }

    /// Returns the URL of the component.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Returns the host of the component, if present.
    pub fn host(&self) -> Option<&str> {
        self.url.host_str()
    }

    /// Returns the path component of the URL.
    pub fn path(&self) -> &str {
        self.url.path()
    }

    /// Returns the protocol and transport parts of the URL.
    ///
    /// For example, for "mcp+http://example.com", returns ("mcp", Some("http")).
    pub fn protocol_transport(&self) -> (&str, Option<&str>) {
        let scheme = self.url.scheme();
        match self.url.scheme().split_once("+") {
            Some((protocol, transport)) => (protocol, Some(transport)),
            None => (scheme, None),
        }
    }

    /// Returns the protocol part of the URL.
    ///
    /// For example, for "mcp+http://example.com", returns "mcp".
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
    /// Returns an error if the string is not a valid URL.
    pub fn parse(url: &str) -> Result<Self, url::ParseError> {
        let url = Url::from_str(url)?;
        Ok(Self::new(url))
    }

    pub fn key(&self) -> ComponentKey<'_> {
        (self.host(), self.path())
    }
}
