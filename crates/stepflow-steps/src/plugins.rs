use std::collections::HashMap;

use crate::{PluginError, Result, StepPlugin};
use error_stack::ResultExt;
use stepflow_workflow::Component;

pub struct Plugins {
    step_plugins: HashMap<String, Box<dyn StepPlugin>>,
}

impl Default for Plugins {
    fn default() -> Self {
        Plugins::new()
    }
}

impl Plugins {
    pub fn new() -> Self {
        Self {
            step_plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: impl StepPlugin) {
        self.step_plugins
            .insert(plugin.protocol().to_owned(), Box::new(plugin));
    }

    pub fn get_dyn(&self, component: &Component) -> Result<&dyn StepPlugin> {
        let protocol = component.protocol();

        let plugin = self
            .step_plugins
            .get(protocol)
            .ok_or_else(|| PluginError::UnknownScheme(protocol.to_owned()))
            .attach_printable_lazy(|| component.clone())?;
        Ok(plugin.as_ref())
    }

    pub fn get<T: StepPlugin>(&self, component: &Component) -> Result<&T> {
        let step_plugin = self.get_dyn(component)?;
        let downcast = step_plugin
            .downcast_ref::<T>()
            .ok_or_else(|| PluginError::DowncastErr(component.protocol().to_owned()))?;

        Ok(downcast)
    }
}

#[cfg(test)]
mod tests {
    use stepflow_workflow::{Step, Value};

    use crate::{ComponentInfo, Result};

    use super::*;

    #[derive(Eq, PartialEq, Debug)]
    struct MockPlugin(&'static str);

    impl StepPlugin for MockPlugin {
        fn protocol(&self) -> &'static str {
            self.0
        }

        fn component_info(&self, _component: &Component) -> Result<ComponentInfo> {
            todo!()
        }

        fn execute(&self, _component: &Component, _args: Vec<Value>) -> Result<Vec<Value>> {
            todo!()
        }
    }

    #[test]
    fn test_plugins() {
        let mut plugins = Plugins::new();
        plugins.register(MockPlugin("langflow"));
        plugins.register(MockPlugin("mcp"));

        let langflow: &MockPlugin = plugins
            .get(&Component::parse("langflow://package/class/name").unwrap())
            .unwrap();
        assert_eq!(langflow, &MockPlugin("langflow"));

        let mcp_over_http: &MockPlugin = plugins
            .get(&Component::parse("mcp+http://package/class/name").unwrap())
            .unwrap();
        assert_eq!(mcp_over_http, &MockPlugin("mcp"));
    }
}
