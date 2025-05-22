use crate::Expr;
use crate::component::Component;
use indexmap::IndexMap;
use schemars::JsonSchema;
use stepflow_schema::ObjectSchema;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StepError {
    #[error("step has no execution info")]
    MissingExecution,
}

/// A step in a workflow that executes a component with specific arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, JsonSchema)]
pub struct Step {
    /// Optional identifier for the step
    pub id: String,

    /// The component to execute in this step
    pub component: Component,

    /// Arguments to pass to the component for this step
    pub args: IndexMap<String, Expr>,

    /// Details related to execution of steps.
    ///
    /// This is filled in prior to executing a workflow. If a workflow
    /// is to be executed many times, the generation of the execution
    /// information (~compilation) may be cached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub execution: Option<StepExecution>,
    // TODO: Optional UI layout information?,
}

/// Details related to execution of steps.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Default, JsonSchema)]
pub struct StepExecution {
    /// The input schema for this step.
    pub input_schema: Option<ObjectSchema>,

    /// The output schema for this step.
    pub output_schema: Option<ObjectSchema>,
}
