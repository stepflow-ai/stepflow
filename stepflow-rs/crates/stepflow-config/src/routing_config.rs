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
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use stepflow_flow::values::ValueRef;
use stepflow_flow::workflow::JsonPath;

/// Prefix-keyed routing configuration.
///
/// Keys are single-segment path prefixes (e.g., "/python", "/builtin") or the
/// root catch-all "/". Multi-segment prefixes (e.g., "/python/core") are not
/// allowed. Each plugin's registered component paths are mounted under the
/// prefix. The orchestrator builds a per-plugin trie from the plugin's
/// component registrations.
#[derive(Serialize, Deserialize, Debug, Clone, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingConfig {
    /// Prefix-to-routing rules mapping.
    ///
    /// Keys must be either "/" (catch-all) or a single-segment prefix like
    /// "/python", "/builtin". Multi-segment prefixes are rejected at build time.
    /// Each plugin's registered component paths are mounted under the prefix.
    ///
    /// Value: ordered list of routing rules. When multiple rules exist for a prefix,
    /// they are evaluated in order — the first rule whose conditions match is used.
    pub routes: HashMap<String, Vec<RouteRule>>,
}

/// A single routing rule mapping a prefix to a plugin.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteRule {
    /// Optional input conditions that must match for this rule to apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<InputCondition>,

    /// Optional component allowlist — only these component IDs are allowed.
    ///
    /// If omitted, all components are allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Vec<String>>")]
    pub component_allow: Option<Vec<Cow<'static, str>>>,

    /// Optional component denylist — these component IDs are blocked.
    ///
    /// If omitted, no components are blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Vec<String>>")]
    pub component_deny: Option<Vec<Cow<'static, str>>>,

    /// Plugin name to route to.
    #[schemars(with = "String")]
    pub plugin: Cow<'static, str>,

    /// Additional parameters passed to the plugin at task dispatch time.
    ///
    /// These are transport-specific overrides (e.g., `stream` for NATS,
    /// `queueName` for gRPC) that the plugin merges with its own defaults.
    /// Unknown fields in route rules are captured here via `#[serde(flatten)]`.
    #[serde(flatten)]
    #[schemars(with = "std::collections::HashMap<String, serde_json::Value>")]
    pub params: std::collections::HashMap<String, serde_json::Value>,
}

/// JSON path condition for matching specific parts of input data
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InputCondition {
    /// JSON path expression (e.g., "$.model", "$.config.temperature")
    pub path: JsonPath,

    /// Value to match against (equality comparison)
    pub value: ValueRef,
}

/// Information about a route match for reverse routing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouteMatch {
    /// The path pattern that matches this route (e.g., "/builtin")
    pub path_pattern: String,
    /// The resolved path for this specific component (e.g., "/builtin/eval")
    pub resolved_path: String,
    /// Input conditions that must be met for this route to be available
    pub conditions: Vec<InputCondition>,
    /// Whether this route is conditional (derived from conditions.is_empty())
    pub is_conditional: bool,
}
