use std::collections::HashMap;

use crate::{DynStepPlugin, PluginError, Result, StepPlugin};
use error_stack::ResultExt;
use stepflow_workflow::Component;

pub struct Plugins<'a> {
    step_plugins: HashMap<&'static str, &'a DynStepPlugin<'a>>,
}

impl Default for Plugins<'_> {
    fn default() -> Self {
        Plugins::new()
    }
}

impl<'a> Plugins<'a> {
    pub fn new() -> Self {
        Self {
            step_plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: &'a impl StepPlugin) {
        let protocol = plugin.protocol();
        let plugin = DynStepPlugin::from_ref(plugin);

        self.step_plugins.insert(protocol, plugin);
    }

    pub fn get(&self, component: &'_ Component) -> Result<&'a DynStepPlugin> {
        let protocol = component.protocol();

        let plugin = self
            .step_plugins
            .get(protocol)
            .ok_or_else(|| PluginError::UnknownScheme(protocol.to_owned()))
            .attach_printable_lazy(|| component.clone())?;
        Ok(plugin)
    }
}

#[cfg(test)]
mod tests {
    use stepflow_workflow::Value;

    use crate::{ComponentInfo, Result};

    use super::*;

    #[derive(Eq, PartialEq, Debug)]
    struct MockPlugin(&'static str);

    impl StepPlugin for MockPlugin {
        fn protocol(&self) -> &'static str {
            self.0
        }

        async fn component_info(&self, _component: &Component) -> Result<ComponentInfo> {
            todo!()
        }

        async fn execute(&self, _component: &Component, _args: Vec<Value>) -> Result<Vec<Value>> {
            todo!()
        }
    }

    #[test]
    fn test_plugins() {
        let mut plugins = Plugins::new();
        let langflow = MockPlugin("langflow");
        let mcp = MockPlugin("mcp");
        plugins.register(&langflow);
        plugins.register(&mcp);

        let langflow = plugins
            .get(&Component::parse("langflow://package/class/name").unwrap())
            .unwrap();
        assert_eq!(langflow.protocol(), "langflow");

        let mcp_over_http = plugins
            .get(&Component::parse("mcp+http://package/class/name").unwrap())
            .unwrap();
        assert_eq!(mcp_over_http.protocol(), "mcp");
    }
}
