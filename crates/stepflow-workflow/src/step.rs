use crate::{Expr, ValueRef};
use crate::component::Component;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StepError {
    #[error("step has no execution info")]
    MissingExecution,
}

/// A reference to a step's output, including the step ID, output name, and optional slot index.
#[derive(
    serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord,
)]
pub struct StepRef {
    /// The ID of the step that produced this output.
    pub step_id: String,
    /// The name of the output from the step
    pub output: String,
}

/// Represents a step output with its name and optional usage count
#[derive(Debug, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
pub struct StepOutput {
    /// The name of the output
    pub name: String,
    /// Optional count of how many times this output is used
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub uses: Option<u32>,
    /// The ValueRef assigned to this step output.
    pub value_ref: Option<ValueRef>,
}

impl StepOutput {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            uses: None,
            value_ref: None,
        }
    }
}

/// A step in a workflow that executes a component with specific arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
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
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Default)]
pub struct StepExecution {
    /// Information about the step's outputs.
    ///
    /// Not all outputs of the component need to be named
    /// in the `outputs` of a step. In that case, it indicates
    /// the unnamed outputs aren't needed.
    pub outputs: Vec<StepOutput>,

    /// Whether this step should execute if none of its outputs are used.
    pub always_execute: bool,
}

impl Step {
    /// Returns true if any of the step's outputs are used in the workflow
    pub fn used(&self) -> bool {
        self.execution
            .as_ref()
            .expect("missing execution info")
            .outputs
            .iter()
            .any(|output| output.uses.unwrap_or(0) > 0)
    }
}
