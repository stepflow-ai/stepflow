use crate::{BuiltinComponent, registry};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{Plugin, PluginConfig, PluginError, Result};

/// The struct that implements the `Plugin` trait.
#[derive(Default)]
pub struct Builtins;

#[derive(Serialize, Deserialize, Debug)]
pub struct BuiltinPluginConfig;

impl PluginConfig for BuiltinPluginConfig {
    type Plugin = Builtins;
    type Error = PluginError;

    async fn create_plugin(
        self,
        _working_directory: &std::path::Path,
    ) -> error_stack::Result<Self::Plugin, Self::Error> {
        Ok(Builtins::new())
    }
}

impl Builtins {
    /// Create a new instance with default components registered
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for Builtins {
    async fn init(&self) -> Result<()> {
        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let component = registry::get_component(component)?;
        component
            .component_info()
            .change_context(PluginError::ComponentInfo)
    }

    async fn execute(&self, component: &Component, input: ValueRef) -> Result<FlowResult> {
        let component = registry::get_component(component)?;
        component
            .execute(input)
            .await
            .change_context(PluginError::UdfExecution)
    }
}
