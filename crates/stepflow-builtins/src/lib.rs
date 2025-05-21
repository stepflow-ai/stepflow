use stepflow_protocol::component_info::ComponentInfo;
use stepflow_workflow::Value;

mod error;
mod openai;
mod plugin;
mod registry;

use error::Result;
pub use plugin::Builtins;

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(DynBuiltinComponent = dyn BuiltinComponent)]
pub(crate) trait BuiltinComponent: Send + Sync {
    fn component_info(&self) -> Result<ComponentInfo>;

    async fn execute(&self, input: Value) -> Result<Value>;
}
