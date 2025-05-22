use crate::{BuiltinComponent, registry};
use error_stack::ResultExt as _;
use stepflow_plugin::{Plugin, PluginError, Result};
use stepflow_protocol::component_info::ComponentInfo;
use stepflow_workflow::{Component, Value};

/// The struct that implements the `Plugin` trait.
#[derive(Default)]
pub struct Builtins;

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

    async fn execute(&self, component: &Component, input: Value) -> Result<Value> {
        let component = registry::get_component(component)?;
        component
            .execute(input)
            .await
            .change_context(PluginError::UdfExecution)
    }
}
