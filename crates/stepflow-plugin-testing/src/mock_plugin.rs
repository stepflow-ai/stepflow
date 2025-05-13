use std::collections::HashMap;

use stepflow_plugin::{Plugin, PluginError, Result};
use stepflow_protocol::component_info::ComponentInfo;
use stepflow_schema::ObjectSchema;
use stepflow_workflow::{Component, Value};

#[derive(Debug, PartialEq, Eq)]
pub struct MockPlugin {
    kind: &'static str,
    components: HashMap<Component, MockComponent>,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct MockComponent {
    always_execute: bool,
}

impl MockComponent {
    pub fn always_execute(&mut self, always_execute: bool) -> &mut Self {
        self.always_execute = always_execute;
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
            input_schema: ObjectSchema::default(),
            output_schema: ObjectSchema::default(),
        })
    }

    async fn execute(&self, component: &Component, input: Value) -> Result<Value> {
        todo!()
    }
}
