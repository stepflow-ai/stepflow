use super::{Component, Expr};
use indexmap::IndexMap;
use schemars::JsonSchema;
use crate::schema::ObjectSchema;

/// A step in a workflow that executes a component with specific arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, JsonSchema)]
pub struct Step {
    /// Optional identifier for the step
    pub id: String,

    /// The component to execute in this step
    pub component: Component,

    /// Arguments to pass to the component for this step
    pub args: IndexMap<String, Expr>,
}

/// Details related to execution of steps.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Default, JsonSchema)]
pub struct StepExecution {
    /// The input schema for this step.
    pub input_schema: Option<ObjectSchema>,

    /// The output schema for this step.
    pub output_schema: Option<ObjectSchema>,
}
