use std::{path::Path, sync::Arc};

use crate::{Context, ExecutionContext, Result};
use serde::{Serialize, de::DeserializeOwned};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(pub DynPlugin = dyn Plugin)]
pub trait Plugin: Send + Sync {
    async fn init(&self, context: &Arc<dyn Context>) -> Result<()>;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    async fn execute(
        &self,
        component: &Component,
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult>;
}

/// Trait implemented by a deserializable plugin configuration.
pub trait PluginConfig: Serialize + DeserializeOwned {
    type Error: error_stack::Context;

    fn create_plugin(
        self,
        working_directory: &Path,
    ) -> impl Future<Output = error_stack::Result<Box<DynPlugin<'static>>, Self::Error>>;
}
