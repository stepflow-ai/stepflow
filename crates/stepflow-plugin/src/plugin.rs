use crate::Result;
use stepflow_core::{component::ComponentInfo, workflow::{Component, ValueRef}};

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(pub DynPlugin = dyn Plugin)]
pub trait Plugin: Send + Sync {
    async fn init(&self) -> Result<()>;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    async fn execute(&self, component: &Component, input: ValueRef) -> Result<ValueRef>;
}
