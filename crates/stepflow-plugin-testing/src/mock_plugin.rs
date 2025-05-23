use std::collections::HashMap;

use stepflow_core::{
    component::ComponentInfo,
    schema::SchemaRef,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Plugin, PluginError, Result};

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
    Valid { output: ValueRef },
}

impl MockComponentBehavior {
    pub fn valid(output: impl Into<ValueRef>) -> Self {
        Self::Valid {
            output: output.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct MockComponent {
    input_schema: SchemaRef,
    output_schema: SchemaRef,
    behaviors: HashMap<ValueRef, MockComponentBehavior>,
}

impl Default for MockComponent {
    fn default() -> Self {
        Self {
            input_schema: SchemaRef::parse_json(r#"{"type": "object"}"#).unwrap(),
            output_schema: SchemaRef::parse_json(r#"{"type": "object"}"#).unwrap(),
            behaviors: HashMap::new(),
        }
    }
}

impl MockComponent {
    pub fn input_schema(&mut self, input_schema: SchemaRef) -> &mut Self {
        self.input_schema = input_schema;
        self
    }

    pub fn output_schema(&mut self, output_schema: SchemaRef) -> &mut Self {
        self.output_schema = output_schema;
        self
    }

    pub fn behavior(
        &mut self,
        input: impl Into<ValueRef>,
        behavior: MockComponentBehavior,
    ) -> &mut Self {
        self.behaviors.insert(input.into(), behavior);
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
    async fn init(&self) -> Result<()> {
        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let component = self
            .components
            .get(component)
            .ok_or(PluginError::UdfImport)?;
        Ok(ComponentInfo {
            input_schema: component.input_schema.clone(),
            output_schema: component.output_schema.clone(),
        })
    }

    async fn execute(&self, component: &Component, input: ValueRef) -> Result<ValueRef> {
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
