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
//! Types are re-exported from [`stepflow_flow`] — the canonical definitions shared
//! with the orchestrator. The [`FlowBuilder`] provides a convenient API for
//! constructing flows programmatically.
//!
//! # Quick start
//!
//! ```rust
//! use stepflow_client::{FlowBuilder, ValueExpr};
//! use stepflow_flow::values::JsonPath;
//!
//! let mut builder = FlowBuilder::new();
//! builder.add_step(
//!     "say_hello",
//!     "/builtin/eval",
//!     ValueExpr::object(vec![("message".to_string(), ValueExpr::workflow_input(JsonPath::parse("$.name").unwrap()))]),
//! );
//! let flow = builder
//!     .output(ValueExpr::step_output("say_hello"))
//!     .build()
//!     .unwrap();
//!
//! let json = serde_json::to_value(&flow).unwrap();
//! ```

use std::collections::HashMap;

use crate::error::BuilderError;

// Re-export the canonical types from stepflow-flow.
pub use stepflow_flow::ValueExpr;
pub use stepflow_flow::values::{JsonPath, ValueRef};
pub use stepflow_flow::workflow::{
    Component, ErrorAction, ExampleInput, Flow, FlowRef, FlowSchema, Step, TestCase, TestConfig,
};

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
/// let mut builder = FlowBuilder::new();
/// builder.add_step(
///     "summarize",
///     "/builtin/openai",
///     ValueExpr::object(vec![
///         ("model".to_string(), ValueExpr::literal(serde_json::json!("gpt-4o"))),
///     ]),
/// );
/// let flow = builder
///     .output(ValueExpr::step_output("summarize"))
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

    /// Add a step to the flow.
    ///
    /// # Panics
    ///
    /// Panics if a step with the same ID already exists.
    pub fn add_step(
        &mut self,
        id: impl Into<String>,
        component: impl Into<String>,
        input: impl Into<ValueExpr>,
    ) {
        self.try_add_step(id, component, input)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    /// Add a step to the flow, returning an error on duplicate IDs.
    pub fn try_add_step(
        &mut self,
        id: impl Into<String>,
        component: impl Into<String>,
        input: impl Into<ValueExpr>,
    ) -> Result<(), BuilderError> {
        let id = id.into();
        if self.step_ids.contains(&id) {
            return Err(BuilderError::DuplicateStep(id));
        }
        self.step_ids.insert(id.clone());
        self.steps.push(Step {
            id,
            component: Component::from_string(component.into()),
            input: input.into(),
            on_error: None,
            must_execute: None,
            metadata: HashMap::new(),
        });
        Ok(())
    }

    /// Set the flow output expression.
    pub fn output(mut self, expr: impl Into<ValueExpr>) -> Self {
        self.output = Some(expr.into());
        self
    }

    /// Build the [`Flow`].
    pub fn build(self) -> Result<Flow, BuilderError> {
        Ok(Flow {
            name: self.name,
            description: self.description,
            version: self.version,
            steps: self.steps,
            output: self.output.unwrap_or_else(ValueExpr::null),
            metadata: self.metadata,
            ..Default::default()
        })
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
    fn test_flow_builder() {
        let mut builder = FlowBuilder::new();
        builder.add_step("greet", "/builtin/eval", ValueExpr::null());
        let flow = builder
            .output(ValueExpr::step_output("greet"))
            .build()
            .unwrap();

        assert_eq!(flow.steps.len(), 1);
        assert_eq!(flow.steps[0].id, "greet");

        let json = serde_json::to_value(&flow).unwrap();
        assert_eq!(json["steps"][0]["id"], "greet");
        assert_eq!(json["output"]["$step"], "greet");
    }

    #[test]
    fn test_flow_serialization() {
        let mut builder = FlowBuilder::new().name("test");
        builder.add_step(
            "step1",
            "/builtin/eval",
            ValueExpr::object(vec![(
                "model".to_string(),
                ValueExpr::literal(json!("gpt-4o")),
            )]),
        );
        let flow = builder
            .output(ValueExpr::step_output("step1"))
            .build()
            .unwrap();

        let json = serde_json::to_value(&flow).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["steps"][0]["component"], "/builtin/eval");
    }
}
