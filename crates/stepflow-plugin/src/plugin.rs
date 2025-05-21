use crate::Result;
use stepflow_protocol::component_info::ComponentInfo;
use stepflow_workflow::{Component, Value};

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(pub DynPlugin = dyn Plugin)]
pub trait Plugin: Send + Sync {
    async fn init(&self) -> Result<()>;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    async fn execute(&self, component: &Component, input: Value) -> Result<Value>;
}
