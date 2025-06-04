use std::sync::Arc;

use crate::{BuiltinComponent as _, registry};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result,
};

/// The struct that implements the `Plugin` trait.
#[derive(Default)]
pub struct Builtins;

#[derive(Serialize, Deserialize, Debug)]
pub struct BuiltinPluginConfig;

impl PluginConfig for BuiltinPluginConfig {
    type Error = PluginError;

    async fn create_plugin(
        self,
        _working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        Ok(DynPlugin::boxed(Builtins::new()))
    }
}

impl Builtins {
    /// Create a new instance with default components registered
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for Builtins {
    async fn init(&self, _context: &Arc<dyn Context>) -> Result<()> {
        Ok(())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let component = registry::get_component(component)?;
        component
            .component_info()
            .change_context(PluginError::ComponentInfo)
    }

    async fn execute(
        &self,
        component: &Component,
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let component = registry::get_component(component)?;
        component
            .execute(context, input)
            .await
            .change_context(PluginError::UdfExecution)
    }
}
