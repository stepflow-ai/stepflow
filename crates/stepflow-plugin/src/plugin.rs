use crate::Result;
use stepflow_workflow::{Component, Value};

pub struct ComponentInfo {
    pub always_execute: bool,
}

#[trait_variant::make(SendPlugin: Send)]
#[dynosaur::dynosaur(pub DynPlugin = dyn Plugin)]
#[dynosaur::dynosaur(pub DynSendPlugin = dyn SendPlugin)]
pub trait Plugin {
    fn protocol(&self) -> &'static str;

    async fn init(&self) -> Result<()>;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    async fn execute(&self, component: &Component, input: Value) -> Result<Value>;
}
