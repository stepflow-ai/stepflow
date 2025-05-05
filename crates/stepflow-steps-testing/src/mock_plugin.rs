use std::collections::HashMap;

use stepflow_steps::{ComponentInfo, PluginError, Result, StepPlugin};
use stepflow_workflow::{Component, StepOutput, Value};

#[derive(Debug, PartialEq, Eq)]
pub struct MockPlugin {
    kind: &'static str,
    components: HashMap<Component, MockComponent>,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct MockComponent {
    always_execute: bool,
    outputs: Vec<StepOutput>,
}

impl MockComponent {
    pub fn always_execute(&mut self, always_execute: bool) -> &mut Self {
        self.always_execute = always_execute;
        self
    }

    pub fn outputs(&mut self, names: &[&str]) -> &mut Self {
        for name in names {
            self.outputs.push(StepOutput::new(name));
        }
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
        println!("Mock component: {:?}", component);
        self.components.entry(component).or_default()
    }
}

impl StepPlugin for MockPlugin {
    fn protocol(&self) -> &'static str {
        self.kind
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let component = self
            .components
            .get(component)
            .ok_or(PluginError::UdfImport)?;
        Ok(ComponentInfo {
            always_execute: component.always_execute,
            outputs: component.outputs.clone(),
        })
    }

    async fn execute(&self, component: &Component, args: Vec<Value>) -> Result<Vec<Value>> {
        todo!()
    }
}
