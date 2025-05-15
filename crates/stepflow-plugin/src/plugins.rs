use std::{collections::HashMap, sync::Arc};

use crate::{DynPlugin, Plugin, PluginError, Result};
use error_stack::ResultExt;
use stepflow_workflow::Component;

pub struct Plugins {
    step_plugins: HashMap<&'static str, Arc<DynPlugin<'static>>>,
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

    pub fn register<P: Plugin + 'static>(&mut self, plugin: P) {
        let protocol = plugin.protocol();
        let plugin = DynPlugin::boxed(plugin);
        let plugin: Arc<DynPlugin<'static>> = Arc::from(plugin);
        self.step_plugins.insert(protocol, plugin);
    }

    pub fn get(&self, component: &'_ Component) -> Result<Arc<DynPlugin<'static>>> {
        let protocol = component.protocol();

        let plugin = self
            .step_plugins
            .get(protocol)
            .ok_or_else(|| PluginError::UnknownScheme(protocol.to_owned()))
            .attach_printable_lazy(|| component.clone())?;
        Ok(plugin.clone())
    }
}

#[cfg(test)]
mod tests {
    use stepflow_protocol::component_info::ComponentInfo;
    use stepflow_workflow::Value;

    use crate::{Plugin, Result};

    use super::*;

    #[derive(Eq, PartialEq, Debug)]
    struct MockPlugin(&'static str);

    impl Plugin for MockPlugin {
        fn protocol(&self) -> &'static str {
            self.0
        }

        async fn init(&self) -> Result<()> {
            Ok(())
        }

        async fn component_info(&self, _component: &Component) -> Result<ComponentInfo> {
            todo!()
        }

        async fn execute(&self, _component: &Component, _input: Value) -> Result<Value> {
            todo!()
        }
    }

    #[test]
    fn test_plugins() {
        let mut plugins = Plugins::new();
        plugins.register(MockPlugin("langflow"));
        plugins.register(MockPlugin("mcp"));

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
