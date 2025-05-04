use std::fmt;

use crate::component::Component;
use crate::Expr;
use indexmap::IndexMap;
use serde::Deserialize;
use serde::ser::SerializeStruct as _;

/// A reference to a step's output, including the step ID, output name, and optional slot index.
#[derive(
    serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord,
)]
pub struct StepRef {
    /// The ID of the step that produced this output.
    pub step_id: String,
    /// The name of the output from the step
    pub output: String,
    /// Optional slot index used for this value
    pub slot: Option<u32>,
}

/// Represents a step output with its name and optional usage count
#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub struct StepOutput {
    /// The name of the output
    pub name: String,
    /// Optional count of how many times this output is used
    pub uses: Option<u32>,
}

impl serde::Serialize for StepOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.uses.is_none() {
            serializer.serialize_str(&self.name)
        } else {
            let mut output = serializer.serialize_struct("StepOutput", 3)?;
            output.serialize_field("name", &self.name)?;
            output.serialize_field("uses", &self.uses)?;
            output.end()
        }
    }
}

impl<'de> serde::Deserialize<'de> for StepOutput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StepOutputVisitor)
    }
}

struct StepOutputVisitor;

impl<'de> serde::de::Visitor<'de> for StepOutputVisitor {
    type Value = StepOutput;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string or step output")
    }

    fn visit_str<E>(self, value: &str) -> Result<StepOutput, E>
    where
        E: serde::de::Error,
    {
        Ok(StepOutput {
            name: value.to_owned(),
            uses: None,
        })
    }

    fn visit_map<M>(self, map: M) -> Result<StepOutput, M::Error>
    where
        M: serde::de::MapAccess<'de>,
    {
        Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(map))
    }
}

/// A step in a workflow that executes a component with specific arguments.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct Step {
    /// Optional identifier for the step
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,

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
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct StepExecution {
    /// Information about the step's outputs.
    ///
    /// Not all outputs of the component need to be named
    /// in the `outputs` of a step. In that case, it indicates
    /// the unnamed outputs aren't needed.
    pub outputs: Vec<StepOutput>,

    /// Whether this step should execute if none of its outputs are used.
    pub always_execute: bool,

    /// Slots which can be dropped after this step.
    pub drop_slots: Vec<u32>,
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
