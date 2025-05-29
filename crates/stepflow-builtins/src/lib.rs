use std::sync::Arc;
use stepflow_core::{FlowResult, component::ComponentInfo, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;

mod error;
mod eval;
mod load_file;
mod messages;
#[cfg(test)]
mod mock_context;
mod openai;
mod plugin;
mod registry;

use error::Result;
pub use plugin::{BuiltinPluginConfig, Builtins};

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(DynBuiltinComponent = dyn BuiltinComponent)]
pub(crate) trait BuiltinComponent: Send + Sync {
    fn component_info(&self) -> Result<ComponentInfo>;

    async fn execute(
        &self,
        context: Arc<dyn ExecutionContext>,
        input: ValueRef,
    ) -> Result<FlowResult>;
}
