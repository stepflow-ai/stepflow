use crate::Result;
use stepflow_workflow::{Component, StepOutput, Value};

pub struct ComponentInfo {
    pub always_execute: bool,
    pub outputs: Vec<StepOutput>,
}

#[trait_variant::make(SendStepPlugin: Send)]
#[dynosaur::dynosaur(pub DynStepPlugin = dyn StepPlugin)]
#[dynosaur::dynosaur(pub DynSendStepPlugin = dyn SendStepPlugin)]
pub trait StepPlugin {
    fn protocol(&self) -> &'static str;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    async fn execute(&self, component: &Component, args: Vec<Value>) -> Result<Vec<Value>>;
}
