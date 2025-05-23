use std::{collections::HashMap, sync::Arc};

use crate::{DynPlugin, Plugin, PluginError, Result};
use error_stack::ResultExt;
use stepflow_core::workflow::Component;

pub struct Plugins {
    step_plugins: HashMap<String, Arc<DynPlugin<'static>>>,
    initialized: bool,
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
            initialized: false,
        }
    }

    pub fn register<P: Plugin + 'static>(&mut self, protocol: String, plugin: P) -> Result<()> {
        error_stack::ensure!(!self.initialized, PluginError::AlreadyInitialized);
        let plugin = DynPlugin::boxed(plugin);
        let plugin: Arc<DynPlugin<'static>> = Arc::from(plugin);
        self.step_plugins.insert(protocol, plugin);
        Ok(())
    }

    pub async fn initialize(&mut self) -> Result<()> {
        error_stack::ensure!(!self.initialized, PluginError::AlreadyInitialized);
        self.initialized = true;

        let init_tasks = self
            .step_plugins
            .values()
            .map(|plugin| plugin.init())
            .collect::<Vec<_>>();
        futures::future::try_join_all(init_tasks).await?;

        Ok(())
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
    use stepflow_core::component::ComponentInfo;
    use stepflow_core::workflow::Value;

    use crate::{Plugin, Result};

    use super::*;

    #[derive(Eq, PartialEq, Debug)]
    struct MockPlugin(&'static str);

    impl Plugin for MockPlugin {
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
        plugins
            .register("langflow".to_owned(), MockPlugin("langflow"))
            .unwrap();
        plugins
            .register("mcp".to_owned(), MockPlugin("mcp"))
            .unwrap();

        plugins
            .get(&Component::parse("langflow://package/class/name").unwrap())
            .unwrap();

        plugins
            .get(&Component::parse("mcp+http://package/class/name").unwrap())
            .unwrap();
    }
}
