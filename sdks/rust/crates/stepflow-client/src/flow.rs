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

//! Flow definition types and programmatic builder API.
//!
//! # Quick start
//!
//! ```rust
//! use stepflow_client::{FlowBuilder, ValueExpr};
//!
//! let mut builder = FlowBuilder::new().name("greet");
//! builder.add_step(
//!     "say_hello",
//!     "/builtin/eval",
//!     ValueExpr::object([("message", FlowBuilder::input().field("name").into())]),
//! );
//! let flow = builder
//!     .output(FlowBuilder::step("say_hello"))
//!     .build()
//!     .unwrap();
//!
//! let json = flow.to_json().unwrap();
//! ```

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::BuilderError;

// ---------------------------------------------------------------------------
// Value expressions
// ---------------------------------------------------------------------------

/// A value expression used in flow step inputs and flow outputs.
///
/// Expressions can be literal JSON values, references to step outputs, references to
/// flow inputs, or references to flow variables.
///
/// # Serialization
///
/// `ValueExpr` serializes to the Stepflow JSON wire format:
/// - Literals serialize directly: `true`, `42`, `"hello"`, `[...]`, `{...}`
/// - Step references: `{"$step": "step_id"}` or `{"$step": "step_id", "path": "$.field"}`
/// - Input references: `{"$input": "$.field"}` or `{"$input": "$"}` for root
/// - Variable references: `{"$variable": "$.var_name"}`
/// - Escaped literals: `{"$literal": <value>}` — prevents inner `$` keys from being
///   interpreted as references
#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    /// A literal JSON null.
    Null,
    /// A literal JSON boolean.
    Bool(bool),
    /// A literal JSON number.
    Number(serde_json::Number),
    /// A literal JSON string.
    String(String),
    /// A literal JSON array whose elements may themselves be expressions.
    Array(Vec<ValueExpr>),
    /// A literal JSON object whose values may themselves be expressions.
    Object(IndexMap<String, ValueExpr>),
    /// A reference to a previous step's output.
    StepRef {
        /// Step ID to reference.
        step: String,
        /// Optional JSONPath within the step's output, e.g. `"$.result"`.
        /// Use `None` or `""` to reference the entire output.
        path: Option<String>,
    },
    /// A reference to the flow's input.
    InputRef {
        /// Optional JSONPath within the input, e.g. `"$.name"`.
        /// Use `None` or `"$"` to reference the entire input.
        path: Option<String>,
    },
    /// A reference to a flow variable.
    VariableRef {
        /// Variable name (normalized to `$.name` format internally).
        name: String,
        /// Optional default value if the variable is not set.
        default: Option<Box<ValueExpr>>,
    },
    /// An escaped literal value — the inner JSON is passed through verbatim without
    /// interpreting any `$` keys as references. Use this when storing a flow definition
    /// or other JSON that itself contains `$step`/`$input` keys.
    Literal(serde_json::Value),
}

impl ValueExpr {
    /// Create a `ValueExpr::Null`.
    pub fn null() -> Self {
        Self::Null
    }

    /// Create a `ValueExpr::Bool`.
    pub fn bool(v: bool) -> Self {
        Self::Bool(v)
    }

    /// Create a `ValueExpr::Number` from any JSON-number-compatible value.
    pub fn number(v: impl Into<serde_json::Number>) -> Self {
        Self::Number(v.into())
    }

    /// Create a `ValueExpr::String`.
    pub fn string(v: impl Into<String>) -> Self {
        Self::String(v.into())
    }

    /// Create a `ValueExpr::Array`.
    pub fn array(items: impl IntoIterator<Item = ValueExpr>) -> Self {
        Self::Array(items.into_iter().collect())
    }

    /// Create a `ValueExpr::Object` from key-value pairs.
    pub fn object(pairs: impl IntoIterator<Item = (impl Into<String>, ValueExpr)>) -> Self {
        Self::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Create an escaped literal — the value is passed through without interpretation.
    pub fn literal(v: serde_json::Value) -> Self {
        Self::Literal(v)
    }

    /// Create a step reference with an optional JSONPath.
    pub fn step(id: impl Into<String>, path: impl Into<Option<String>>) -> Self {
        Self::StepRef {
            step: id.into(),
            path: path.into(),
        }
    }

    /// Create an input reference with an optional JSONPath.
    pub fn input(path: impl Into<Option<String>>) -> Self {
        Self::InputRef { path: path.into() }
    }

    /// Create a variable reference with an optional default.
    pub fn variable(name: impl Into<String>, default: impl Into<Option<ValueExpr>>) -> Self {
        Self::VariableRef {
            name: name.into(),
            default: default.into().map(Box::new),
        }
    }

    /// Normalize a path string to JSONPath format.
    ///
    /// - `""` → kept as-is (means "root" for inputs)
    /// - `"$"` → kept as-is
    /// - `"$.field"` → kept as-is
    /// - `"field"` → `"$.field"`
    fn normalize_path(path: &str) -> String {
        if path.is_empty() || path == "$" || path.starts_with("$.") || path.starts_with("$[") {
            path.to_string()
        } else {
            format!("$.{path}")
        }
    }
}

impl Serialize for ValueExpr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap as _;

        match self {
            ValueExpr::Null => serializer.serialize_none(),
            ValueExpr::Bool(b) => serializer.serialize_bool(*b),
            ValueExpr::Number(n) => n.serialize(serializer),
            ValueExpr::String(s) => serializer.serialize_str(s),
            ValueExpr::Array(items) => items.serialize(serializer),
            ValueExpr::Object(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            ValueExpr::StepRef { step, path } => {
                let count = if path.is_some() { 2 } else { 1 };
                let mut map = serializer.serialize_map(Some(count))?;
                map.serialize_entry("$step", step)?;
                if let Some(p) = path {
                    let normalized = Self::normalize_path(p);
                    map.serialize_entry("path", &normalized)?;
                }
                map.end()
            }
            ValueExpr::InputRef { path } => {
                let mut map = serializer.serialize_map(Some(1))?;
                let p = path.as_deref().unwrap_or("$");
                let normalized = if p.is_empty() {
                    "$".to_string()
                } else {
                    Self::normalize_path(p)
                };
                map.serialize_entry("$input", &normalized)?;
                map.end()
            }
            ValueExpr::VariableRef { name, default } => {
                let count = if default.is_some() { 2 } else { 1 };
                let mut map = serializer.serialize_map(Some(count))?;
                let normalized = Self::normalize_path(name);
                map.serialize_entry("$variable", &normalized)?;
                if let Some(def) = default {
                    map.serialize_entry("default", def.as_ref())?;
                }
                map.end()
            }
            ValueExpr::Literal(value) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("$literal", value)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ValueExpr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_value_expr(value).map_err(serde::de::Error::custom)
    }
}

fn parse_value_expr(value: serde_json::Value) -> Result<ValueExpr, String> {
    match value {
        serde_json::Value::Null => Ok(ValueExpr::Null),
        serde_json::Value::Bool(b) => Ok(ValueExpr::Bool(b)),
        serde_json::Value::Number(n) => Ok(ValueExpr::Number(n)),
        serde_json::Value::String(s) => Ok(ValueExpr::String(s)),
        serde_json::Value::Array(arr) => arr
            .into_iter()
            .map(parse_value_expr)
            .collect::<Result<Vec<_>, _>>()
            .map(ValueExpr::Array),
        serde_json::Value::Object(obj) => {
            if obj.contains_key("$literal") {
                let lit = obj.into_iter().next().unwrap().1;
                return Ok(ValueExpr::Literal(lit));
            }
            if let Some(step_val) = obj.get("$step") {
                let step = step_val
                    .as_str()
                    .ok_or("$step must be a string")?
                    .to_string();
                let path = obj.get("path").and_then(|p| p.as_str()).map(str::to_string);
                return Ok(ValueExpr::StepRef { step, path });
            }
            if let Some(input_val) = obj.get("$input") {
                let path = input_val.as_str().map(str::to_string);
                return Ok(ValueExpr::InputRef { path });
            }
            if let Some(var_val) = obj.get("$variable") {
                let name = var_val
                    .as_str()
                    .ok_or("$variable must be a string")?
                    .to_string();
                let default = obj
                    .get("default")
                    .cloned()
                    .map(parse_value_expr)
                    .transpose()?
                    .map(Box::new);
                return Ok(ValueExpr::VariableRef { name, default });
            }
            // Regular object — recurse
            obj.into_iter()
                .map(|(k, v)| parse_value_expr(v).map(|e| (k, e)))
                .collect::<Result<IndexMap<_, _>, _>>()
                .map(ValueExpr::Object)
        }
    }
}

// Handy From impls for ergonomic builder usage
impl From<bool> for ValueExpr {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for ValueExpr {
    fn from(v: i64) -> Self {
        Self::Number(v.into())
    }
}
impl From<i32> for ValueExpr {
    fn from(v: i32) -> Self {
        Self::Number(v.into())
    }
}
impl From<f64> for ValueExpr {
    fn from(v: f64) -> Self {
        Self::Number(serde_json::Number::from_f64(v).unwrap_or_else(|| 0i64.into()))
    }
}
impl From<&str> for ValueExpr {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}
impl From<std::string::String> for ValueExpr {
    fn from(v: std::string::String) -> Self {
        Self::String(v)
    }
}
impl From<serde_json::Value> for ValueExpr {
    fn from(v: serde_json::Value) -> Self {
        Self::Literal(v)
    }
}
impl From<StepRef> for ValueExpr {
    fn from(r: StepRef) -> Self {
        Self::StepRef {
            step: r.step_id,
            path: r.path,
        }
    }
}
impl From<InputRef> for ValueExpr {
    fn from(r: InputRef) -> Self {
        Self::InputRef { path: r.path }
    }
}

// ---------------------------------------------------------------------------
// Path builders
// ---------------------------------------------------------------------------

/// A chainable reference to a step's output.
///
/// Obtained from [`FlowBuilder::step`] or [`StepHandle::output`].
#[derive(Debug, Clone)]
pub struct StepRef {
    step_id: String,
    path: Option<String>,
}

impl StepRef {
    fn new(step_id: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            path: None,
        }
    }

    /// Append a field name to the JSONPath, e.g. `.field("result")` → `"$.result"`.
    pub fn field(mut self, name: &str) -> Self {
        let current = self.path.take().unwrap_or_else(|| "$".to_string());
        self.path = Some(format!("{current}.{name}"));
        self
    }

    /// Append an array index to the JSONPath, e.g. `.index(0)` → `"$[0]"`.
    pub fn index(mut self, i: usize) -> Self {
        let current = self.path.take().unwrap_or_else(|| "$".to_string());
        self.path = Some(format!("{current}[{i}]"));
        self
    }
}

/// A chainable reference to the flow's input.
///
/// Obtained from [`FlowBuilder::input`].
#[derive(Debug, Clone)]
pub struct InputRef {
    path: Option<String>,
}

impl InputRef {
    fn new() -> Self {
        Self { path: None }
    }

    /// Append a field name to the JSONPath, e.g. `.field("name")` → `"$.name"`.
    pub fn field(mut self, name: &str) -> Self {
        let current = self.path.take().unwrap_or_else(|| "$".to_string());
        self.path = Some(format!("{current}.{name}"));
        self
    }

    /// Append an array index to the JSONPath, e.g. `.index(0)` → `"$[0]"`.
    pub fn index(mut self, i: usize) -> Self {
        let current = self.path.take().unwrap_or_else(|| "$".to_string());
        self.path = Some(format!("{current}[{i}]"));
        self
    }
}

// ---------------------------------------------------------------------------
// Flow / Step types
// ---------------------------------------------------------------------------

/// A Stepflow workflow definition.
///
/// Serializes to the Stepflow JSON wire format (camelCase keys) accepted by the
/// `StoreFlow` and `CreateRun` APIs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    /// Human-readable name for the flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional description of the flow's purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional semantic version string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Steps to execute. Steps are executed in parallel when their inputs are satisfied.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,

    /// The flow's output expression, evaluated after all required steps complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<ValueExpr>,

    /// Extensible metadata map for tooling and frameworks.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Flow {
    /// Serialize this flow to a `serde_json::Value` suitable for sending to the orchestrator.
    pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }
}

/// A single step within a [`Flow`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Unique identifier for this step within the flow.
    pub id: String,

    /// Component path to execute, e.g. `"/python/my_func"` or `"/builtin/openai"`.
    pub component: String,

    /// Input expression passed to the component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ValueExpr>,

    /// Error handling behaviour for this step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<ErrorAction>,
}

/// Error handling action for a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ErrorAction {
    /// Fail the flow if this step fails (default).
    Fail,
    /// Use a default value if this step fails.
    UseDefault {
        #[serde(skip_serializing_if = "Option::is_none")]
        default_value: Option<serde_json::Value>,
    },
    /// Retry the step if it fails.
    Retry {
        #[serde(skip_serializing_if = "Option::is_none")]
        max_retries: Option<u32>,
    },
}

// ---------------------------------------------------------------------------
// FlowBuilder
// ---------------------------------------------------------------------------

/// Programmatically build a [`Flow`] definition.
///
/// # Example
///
/// ```rust
/// use stepflow_client::{FlowBuilder, ValueExpr};
///
/// let mut builder = FlowBuilder::new().name("summarize");
/// builder.add_step(
///     "summarize",
///     "/builtin/openai",
///     ValueExpr::object([
///         ("messages", ValueExpr::array([
///             ValueExpr::object([
///                 ("role", "user".into()),
///                 ("content", FlowBuilder::input().field("text").into()),
///             ])
///         ])),
///     ]),
/// );
/// let flow = builder
///     .output(FlowBuilder::step("summarize").field("content"))
///     .build()
///     .unwrap();
/// ```
#[derive(Debug, Default)]
pub struct FlowBuilder {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    steps: Vec<Step>,
    step_ids: std::collections::HashSet<String>,
    output: Option<ValueExpr>,
    metadata: HashMap<String, serde_json::Value>,
}

impl FlowBuilder {
    /// Create a new, empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the flow name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the flow description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the flow version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add a metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Add a step to the flow, returning a [`StepHandle`] for building step output references.
    ///
    /// # Panics
    ///
    /// Panics if a step with the same ID already exists. Use [`try_add_step`](Self::try_add_step)
    /// if you need to handle duplicates without panicking.
    pub fn add_step(
        &mut self,
        id: impl Into<String>,
        component: impl Into<String>,
        input: impl Into<Option<ValueExpr>>,
    ) -> StepHandle<'_> {
        self.try_add_step(id, component, input)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// Add a step to the flow, returning a [`StepHandle`] or an error on duplicate IDs.
    ///
    /// Prefer this over [`add_step`](Self::add_step) when you want to handle duplicate step IDs
    /// as a recoverable error rather than a panic.
    pub fn try_add_step(
        &mut self,
        id: impl Into<String>,
        component: impl Into<String>,
        input: impl Into<Option<ValueExpr>>,
    ) -> Result<StepHandle<'_>, crate::error::BuilderError> {
        let id = id.into();
        if self.step_ids.contains(&id) {
            return Err(crate::error::BuilderError::DuplicateStep(id));
        }
        self.step_ids.insert(id.clone());
        self.steps.push(Step {
            id: id.clone(),
            component: component.into(),
            input: input.into(),
            on_error: None,
        });
        Ok(StepHandle {
            step_id: id,
            builder: self,
        })
    }

    /// Set the flow output expression.
    pub fn output(mut self, expr: impl Into<ValueExpr>) -> Self {
        self.output = Some(expr.into());
        self
    }

    /// Build the [`Flow`], returning an error for invalid configurations.
    pub fn build(self) -> Result<Flow, BuilderError> {
        Ok(Flow {
            name: self.name,
            description: self.description,
            version: self.version,
            steps: self.steps,
            output: self.output,
            metadata: self.metadata,
        })
    }

    // -----------------------------------------------------------------------
    // Convenience constructors for value expressions
    // -----------------------------------------------------------------------

    /// Create a [`StepRef`] pointing to the output of the step with the given ID.
    ///
    /// Use `.field("name")` and `.index(0)` to navigate into the output.
    pub fn step(id: impl Into<String>) -> StepRef {
        StepRef::new(id)
    }

    /// Create an [`InputRef`] pointing to the flow's input.
    ///
    /// Use `.field("name")` and `.index(0)` to navigate into the input.
    pub fn input() -> InputRef {
        InputRef::new()
    }

    /// Create a `ValueExpr::Literal` wrapping a `serde_json::Value`.
    pub fn literal(v: serde_json::Value) -> ValueExpr {
        ValueExpr::Literal(v)
    }
}

/// A handle to a step that was just added to a [`FlowBuilder`].
///
/// Use `.output()` to get a [`StepRef`] for referencing this step's output in
/// subsequent steps or the flow output.
pub struct StepHandle<'a> {
    step_id: String,
    builder: &'a mut FlowBuilder,
}

impl<'a> StepHandle<'a> {
    /// Get a [`StepRef`] for this step's output.
    pub fn output(&self) -> StepRef {
        StepRef::new(&self.step_id)
    }

    /// Set the error action for this step.
    pub fn on_error(self, action: ErrorAction) -> Self {
        if let Some(step) = self.builder.steps.iter_mut().find(|s| s.id == self.step_id) {
            step.on_error = Some(action);
        }
        self
    }

    /// Give back the builder so the chaining pattern can continue.
    pub fn done(self) -> &'a mut FlowBuilder {
        self.builder
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_step_ref_serialization() {
        let expr: ValueExpr = StepRef::new("my_step").into();
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$step": "my_step"}));
    }

    #[test]
    fn test_step_ref_with_path() {
        let expr: ValueExpr = StepRef::new("my_step").field("result").into();
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$step": "my_step", "path": "$.result"}));
    }

    #[test]
    fn test_step_ref_with_index() {
        let expr: ValueExpr = StepRef::new("my_step").index(0).field("name").into();
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$step": "my_step", "path": "$[0].name"}));
    }

    #[test]
    fn test_input_ref_root() {
        let expr: ValueExpr = InputRef::new().into();
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$input": "$"}));
    }

    #[test]
    fn test_input_ref_with_field() {
        let expr: ValueExpr = InputRef::new().field("name").into();
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$input": "$.name"}));
    }

    #[test]
    fn test_variable_ref() {
        let expr = ValueExpr::variable("api_key", None);
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$variable": "$.api_key"}));
    }

    #[test]
    fn test_literal() {
        let expr = ValueExpr::literal(json!({"key": "value"}));
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(json, json!({"$literal": {"key": "value"}}));
    }

    #[test]
    fn test_object_with_mixed_values() {
        let expr = ValueExpr::object([
            ("model", "gpt-4o".into()),
            ("messages", StepRef::new("create_messages").into()),
        ]);
        let json = serde_json::to_value(&expr).unwrap();
        assert_eq!(
            json,
            json!({
                "model": "gpt-4o",
                "messages": {"$step": "create_messages"}
            })
        );
    }

    #[test]
    fn test_flow_builder() {
        let mut builder = FlowBuilder::new();
        let step1 = builder.add_step("greet", "/builtin/eval", None);
        let step1_out = step1.output();
        let flow = builder.output(step1_out).build().unwrap();

        assert_eq!(flow.steps.len(), 1);
        assert_eq!(flow.steps[0].id, "greet");

        let json = flow.to_json().unwrap();
        assert_eq!(json["steps"][0]["id"], "greet");
        assert_eq!(json["output"]["$step"], "greet");
    }

    #[test]
    fn test_round_trip() {
        let mut builder = FlowBuilder::new();
        builder.add_step(
            "step1",
            "/python/process",
            ValueExpr::object([
                ("data", InputRef::new().field("payload").into()),
                ("version", 1i64.into()),
            ]),
        );
        let flow = builder
            .output(StepRef::new("step1").field("result"))
            .build()
            .unwrap();

        let json = flow.to_json().unwrap();
        let reconstructed: Flow = serde_json::from_value(json).unwrap();
        assert_eq!(reconstructed.steps.len(), 1);
    }
}
