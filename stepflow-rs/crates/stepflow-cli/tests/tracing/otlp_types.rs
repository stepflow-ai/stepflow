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

//! Minimal OTLP trace types for test verification
//!
//! These types model only the fields we need to verify in tests, not the complete OTLP spec.
//! Based on OpenTelemetry Protocol specification for traces.

use serde::Deserialize;
use std::fmt;

/// Root OTLP trace data structure
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpTrace {
    pub resource_spans: Vec<ResourceSpans>,
}

/// Resource-level span container
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpans {
    pub scope_spans: Vec<ScopeSpans>,
}

/// Instrumentation scope container
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeSpans {
    pub spans: Vec<Span>,
}

/// Individual span data
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    /// Hex-encoded trace ID (32 hex chars for 128-bit ID)
    pub trace_id: String,
    /// Hex-encoded span ID (16 hex chars for 64-bit ID)
    pub span_id: String,
    /// Hex-encoded parent span ID (empty string for root spans)
    #[serde(default)]
    pub parent_span_id: String,
    /// Human-readable span name
    pub name: String,
    /// Span kind (Internal, Server, Client, Producer, Consumer)
    #[serde(default)]
    #[allow(dead_code)]
    pub kind: i32,
    /// Span attributes (key-value pairs)
    #[serde(default)]
    pub attributes: Vec<KeyValue>,
}

/// Key-value attribute for spans
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub key: String,
    pub value: AttributeValue,
}

/// Attribute value wrapper
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeValue {
    #[serde(default)]
    pub string_value: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub int_value: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub bool_value: Option<bool>,
}

impl Span {
    /// Check if this is a root span (no parent)
    pub fn is_root(&self) -> bool {
        self.parent_span_id.is_empty()
    }

    /// Get trace ID as a hex string (lowercased for comparison)
    pub fn trace_id(&self) -> String {
        self.trace_id.to_lowercase()
    }

    /// Get span ID as a hex string (lowercased for comparison)
    pub fn span_id(&self) -> String {
        self.span_id.to_lowercase()
    }

    /// Get parent span ID as a hex string (lowercased for comparison)
    pub fn parent_span_id(&self) -> String {
        self.parent_span_id.to_lowercase()
    }

    /// Check if this span has the given parent span ID
    pub fn has_parent(&self, parent_id: &str) -> bool {
        !self.is_root() && self.parent_span_id() == parent_id.to_lowercase()
    }

    /// Check if this span belongs to the given trace
    pub fn belongs_to_trace(&self, trace_id: &str) -> bool {
        self.trace_id() == trace_id.to_lowercase()
    }

    /// Get a string attribute value by key
    pub fn get_string_attribute(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| kv.value.string_value.as_deref())
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Span {{ name: '{}', span_id: {}, parent: {} }}",
            self.name,
            &self.span_id[..8],
            if self.is_root() {
                "ROOT".to_string()
            } else {
                self.parent_span_id[..8].to_string()
            }
        )
    }
}

impl OtlpTrace {
    /// Extract all spans from this trace data
    pub fn all_spans(&self) -> Vec<&Span> {
        self.resource_spans
            .iter()
            .flat_map(|rs| &rs.scope_spans)
            .flat_map(|ss| &ss.spans)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_is_root() {
        let root_span = Span {
            trace_id: "abc123".to_string(),
            span_id: "def456".to_string(),
            parent_span_id: "".to_string(),
            name: "root".to_string(),
            kind: 0,
            attributes: vec![],
        };
        assert!(root_span.is_root());

        let child_span = Span {
            trace_id: "abc123".to_string(),
            span_id: "ghi789".to_string(),
            parent_span_id: "def456".to_string(),
            name: "child".to_string(),
            kind: 0,
            attributes: vec![],
        };
        assert!(!child_span.is_root());
    }

    #[test]
    fn test_span_has_parent() {
        let span = Span {
            trace_id: "abc123".to_string(),
            span_id: "child123".to_string(),
            parent_span_id: "parent456".to_string(),
            name: "test".to_string(),
            kind: 0,
            attributes: vec![],
        };

        assert!(span.has_parent("parent456"));
        assert!(span.has_parent("PARENT456")); // Case insensitive
        assert!(!span.has_parent("other789"));
    }

    #[test]
    fn test_span_belongs_to_trace() {
        let span = Span {
            trace_id: "abc123def456".to_string(),
            span_id: "span123".to_string(),
            parent_span_id: "".to_string(),
            name: "test".to_string(),
            kind: 0,
            attributes: vec![],
        };

        assert!(span.belongs_to_trace("abc123def456"));
        assert!(span.belongs_to_trace("ABC123DEF456")); // Case insensitive
        assert!(!span.belongs_to_trace("other"));
    }
}
