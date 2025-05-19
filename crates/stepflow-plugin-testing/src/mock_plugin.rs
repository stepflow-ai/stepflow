use std::collections::HashMap;

use stepflow_plugin::{Plugin, PluginError, Result};
use stepflow_protocol::component_info::ComponentInfo;
use stepflow_schema::ObjectSchema;
use stepflow_workflow::{Component, Value};

/// A mock plugin that can be used to test various things in the plugin protocol.
#[derive(Debug, PartialEq)]
pub struct MockPlugin {
    kind: &'static str,
    components: HashMap<Component, MockComponent>,
}

/// Enumeration of behaviors for the mock components.
#[derive(Debug, PartialEq)]
pub enum MockComponentBehavior {
    /// Produce the given error.
    Error { message: String },
    /// Return the given result.
    Valid { output: Value },
}

#[derive(Debug, PartialEq, Default)]
pub struct MockComponent {
    always_execute: bool,
    input_schema: ObjectSchema,
    output_schema: ObjectSchema,
    behaviors: HashMap<Value, MockComponentBehavior>,
}

impl MockComponent {
    pub fn always_execute(&mut self, always_execute: bool) -> &mut Self {
        self.always_execute = always_execute;
        self
    }

    pub fn input_schema(&mut self, input_schema: ObjectSchema) -> &mut Self {
        self.input_schema = input_schema;
        self
    }

    pub fn output_schema(&mut self, output_schema: ObjectSchema) -> &mut Self {
        self.output_schema = output_schema;
        self
    }

    pub fn behavior(&mut self, input: Value, behavior: MockComponentBehavior) -> &mut Self {
        self.behaviors.insert(input, behavior);
        self
    }
}

impl MockPlugin {
    pub fn new(kind: &'static str) -> Self {
        Self {
            kind,
            components: HashMap::new(),
        }
    }

    pub fn mock_component(&mut self, path: &str) -> &mut MockComponent {
        let component = Component::parse(path).unwrap();
        self.components.entry(component).or_default()
    }
}

impl Plugin for MockPlugin {
    fn protocol(&self) -> &'static str {
        self.kind
    }

    async fn init(&self) -> Result<()> {
        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let component = self
            .components
            .get(component)
            .ok_or(PluginError::UdfImport)?;
        Ok(ComponentInfo {
            always_execute: component.always_execute,
            input_schema: component.input_schema.clone(),
            output_schema: component.output_schema.clone(),
        })
    }

    async fn execute(&self, component: &Component, input: Value) -> Result<Value> {
        let component = self
            .components
            .get(component)
            .ok_or(PluginError::UdfImport)?;
        tracing::debug!("Executing component: {:?} on input: {:?}", component, input);
        let output = component
            .behaviors
            .get(&input)
            .ok_or(PluginError::UdfExecution)?;

        match output {
            MockComponentBehavior::Error { .. } => error_stack::bail!(PluginError::UdfExecution),
            MockComponentBehavior::Valid { output } => Ok(output.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static_assertions::assert_impl_all!(MockPlugin: Send, Sync);
}
