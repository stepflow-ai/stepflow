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

use std::collections::HashMap;

use super::{
    Component, ErrorAction, ExampleInput, Flow, FlowV1, JsonPath, Step, TestConfig, VariableSchema,
};
use crate::{ValueExpr, schema::SchemaRef};

/// Builder for creating Flow instances with reduced boilerplate.
#[derive(Default)]
pub struct FlowBuilder {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    input_schema: Option<SchemaRef>,
    output_schema: Option<SchemaRef>,
    steps: Vec<Step>,
    output: Option<ValueExpr>,
    variables: Option<VariableSchema>,
    test: Option<TestConfig>,
    examples: Option<Vec<ExampleInput>>,
    metadata: HashMap<String, serde_json::Value>,
}

impl FlowBuilder {
    /// Create a new FlowBuilder with default values.
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the flow name.
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the flow description.
    pub fn description<S: Into<String>>(mut self, desc: S) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the flow version.
    pub fn version<S: Into<String>>(mut self, version: S) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the input schema.
    pub fn input_schema(mut self, schema: SchemaRef) -> Self {
        self.input_schema = Some(schema);
        self
    }

    /// Set the output schema.
    pub fn output_schema(mut self, schema: SchemaRef) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Add a single step to the flow.
    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    /// Add multiple steps to the flow.
    pub fn steps<I: IntoIterator<Item = Step>>(mut self, steps: I) -> Self {
        self.steps.extend(steps);
        self
    }

    /// Set the flow output.
    pub fn output(mut self, output: ValueExpr) -> Self {
        self.output = Some(output);
        self
    }

    /// Set the variables schema for the flow.
    pub fn variables(mut self, variables: VariableSchema) -> Self {
        self.variables = Some(variables);
        self
    }

    /// Set the test configuration.
    pub fn test_config(mut self, test: TestConfig) -> Self {
        self.test = Some(test);
        self
    }

    /// Set the examples.
    pub fn examples(mut self, examples: Vec<ExampleInput>) -> Self {
        self.examples = Some(examples);
        self
    }

    /// Add metadata.
    pub fn metadata<S: Into<String>>(mut self, key: S, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Create a builder for a test flow with a default name.
    pub fn test_flow() -> Self {
        Self::new().name("test_workflow")
    }

    /// Create a builder for a mock flow with common test defaults.
    pub fn mock_flow() -> Self {
        Self::new()
            .name("mock_flow")
            .description("A test flow for mocking")
    }

    /// Build the final Flow instance.
    pub fn build(self) -> Flow {
        Flow::V1(FlowV1 {
            name: self.name,
            description: self.description,
            version: self.version,
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            variables: self.variables,
            steps: self.steps,
            output: self.output.unwrap_or_default(),
            test: self.test,
            examples: self.examples,
            metadata: self.metadata,
        })
    }
}

/// Builder for creating Step instances with reduced boilerplate.
pub struct StepBuilder {
    id: Option<String>,
    component: Option<Component>,
    input: Option<ValueExpr>,
    input_schema: Option<SchemaRef>,
    output_schema: Option<SchemaRef>,
    skip_if: Option<ValueExpr>,
    on_error: Option<ErrorAction>,
    must_execute: Option<bool>,
    metadata: HashMap<String, serde_json::Value>,
}

impl StepBuilder {
    /// Create a new StepBuilder with the given step ID.
    pub fn new<S: Into<String>>(id: S) -> Self {
        Self {
            id: Some(id.into()),
            component: None,
            input: None,
            input_schema: None,
            output_schema: None,
            skip_if: None,
            on_error: None,
            must_execute: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the component for this step.
    pub fn component<S: Into<String>>(mut self, component: S) -> Self {
        self.component = Some(Component::from_string(component.into()));
        self
    }

    /// Set the input template for this step.
    pub fn input(mut self, input: ValueExpr) -> Self {
        self.input = Some(input);
        self
    }

    /// Set the input from a JSON value, parsing it as a ValueExpr.
    pub fn input_json(mut self, input: serde_json::Value) -> Self {
        self.input = Some(serde_json::from_value(input).unwrap());
        self
    }

    /// Set the input as a literal JSON value.
    pub fn input_literal(mut self, input: serde_json::Value) -> Self {
        self.input = Some(ValueExpr::literal(input));
        self
    }

    /// Set the input schema.
    pub fn input_schema(mut self, schema: SchemaRef) -> Self {
        self.input_schema = Some(schema);
        self
    }

    /// Set the output schema.
    pub fn output_schema(mut self, schema: SchemaRef) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set the skip condition.
    pub fn skip_if(mut self, condition: ValueExpr) -> Self {
        self.skip_if = Some(condition);
        self
    }

    /// Set the error action.
    pub fn on_error(mut self, action: ErrorAction) -> Self {
        self.on_error = Some(action);
        self
    }

    /// Set whether this step must execute even if its output is not used.
    pub fn must_execute(mut self, must_execute: bool) -> Self {
        self.must_execute = Some(must_execute);
        self
    }

    /// Add metadata.
    pub fn metadata<S: Into<String>>(mut self, key: S, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Create a mock step with a default mock component.
    pub fn mock_step<S: Into<String>>(id: S) -> Self {
        Self::new(id).component("/mock/test")
    }

    /// Create a builtin step with the specified builtin component.
    pub fn builtin_step<S: Into<String>>(id: S, component: S) -> Self {
        Self::new(id).component(format!("/builtin/{}", component.into()))
    }

    /// Create a step that references another step's output.
    pub fn step_ref<S: Into<String>>(id: S, ref_step: S) -> Self {
        Self::new(id)
            .component("/mock/test")
            .input(ValueExpr::step_output(ref_step))
    }

    /// Create a step that references workflow input.
    pub fn workflow_input<S: Into<String>>(id: S) -> Self {
        Self::new(id)
            .component("/mock/test")
            .input(ValueExpr::workflow_input(JsonPath::default()))
    }

    /// Build the final Step instance.
    pub fn build(self) -> Step {
        Step {
            id: self.id.expect("Step ID is required"),
            component: self
                .component
                .unwrap_or_else(|| Component::from_string("/mock/test")),
            input: self.input.unwrap_or_else(ValueExpr::null),
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            skip_if: self.skip_if,
            on_error: self.on_error,
            must_execute: self.must_execute,
            metadata: self.metadata,
        }
    }
}

impl Flow {
    /// Create a new FlowBuilder.
    pub fn builder() -> FlowBuilder {
        FlowBuilder::new()
    }
}

impl Step {
    /// Create a new StepBuilder with the given ID.
    pub fn builder<S: Into<String>>(id: S) -> StepBuilder {
        StepBuilder::new(id)
    }
}

/// Create a FlowBuilder for a mock flow with common test defaults.
pub fn mock_flow() -> FlowBuilder {
    FlowBuilder::mock_flow()
}

/// Create a FlowBuilder for a test flow with a default name.
pub fn test_flow() -> FlowBuilder {
    FlowBuilder::test_flow()
}

/// Create a StepBuilder for a mock step.
pub fn mock_step<S: Into<String>>(id: S) -> StepBuilder {
    StepBuilder::mock_step(id)
}

/// Create a StepBuilder for a builtin step.
pub fn builtin_step<S: Into<String>>(id: S, component: S) -> StepBuilder {
    StepBuilder::builtin_step(id, component)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_flow_builder_basic() {
        let flow = FlowBuilder::new()
            .name("test")
            .description("A test flow")
            .build();

        match flow {
            Flow::V1(flow_v1) => {
                assert_eq!(flow_v1.name, Some("test".to_string()));
                assert_eq!(flow_v1.description, Some("A test flow".to_string()));
                assert!(flow_v1.steps.is_empty());
            }
        }
    }

    #[test]
    fn test_flow_builder_with_steps() {
        let step1 = StepBuilder::mock_step("step1")
            .input_literal(json!({"value": 42}))
            .build();

        let step2 = StepBuilder::step_ref("step2", "step1").build();

        let flow = FlowBuilder::test_flow()
            .description("Test with steps")
            .step(step1)
            .step(step2)
            .output(ValueExpr::step_output("step2"))
            .build();

        match flow {
            Flow::V1(flow_v1) => {
                assert_eq!(flow_v1.name, Some("test_workflow".to_string()));
                assert_eq!(flow_v1.steps.len(), 2);
                assert_eq!(flow_v1.steps[0].id, "step1");
                assert_eq!(flow_v1.steps[1].id, "step2");
            }
        }
    }

    #[test]
    fn test_step_builder_basic() {
        let step = StepBuilder::new("test_step")
            .component("/test/component")
            .input_literal(json!({"key": "value"}))
            .build();

        assert_eq!(step.id, "test_step");
        assert_eq!(step.component.path(), "/test/component");
        assert_eq!(step.on_error, None);
        assert_eq!(step.on_error_or_default(), ErrorAction::Fail);
    }

    #[test]
    fn test_step_builder_convenience_methods() {
        let mock_step = StepBuilder::mock_step("mock1").build();
        assert_eq!(mock_step.component.path(), "/mock/test");

        let builtin_step = StepBuilder::builtin_step("builtin1", "openai").build();
        assert_eq!(builtin_step.component.path(), "/builtin/openai");

        let workflow_input_step = StepBuilder::workflow_input("input1").build();
        assert_eq!(workflow_input_step.component.path(), "/mock/test");

        let step_ref = StepBuilder::step_ref("ref1", "step1").build();
        assert_eq!(step_ref.component.path(), "/mock/test");
    }

    #[test]
    fn test_convenience_functions() {
        let flow = mock_flow().step(mock_step("step1").build()).build();

        match flow {
            Flow::V1(flow_v1) => {
                assert_eq!(flow_v1.name, Some("mock_flow".to_string()));
                assert_eq!(flow_v1.steps.len(), 1);
            }
        }
    }
}
