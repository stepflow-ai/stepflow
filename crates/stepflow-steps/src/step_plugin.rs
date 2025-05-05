use std::any::Any;

use crate::Result;
use stepflow_workflow::{Component, Step, StepOutput, Value};

pub struct ComponentInfo {
    pub always_execute: bool,
    pub outputs: Vec<StepOutput>,
}

pub trait StepPlugin: Any {
    fn protocol(&self) -> &'static str;

    /// Return the outputs for the given component.
    fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    fn execute(&self, step: &Step) -> Result<Vec<Value>>;
}

impl dyn StepPlugin {
    pub fn downcast_ref<T: StepPlugin>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }
}
