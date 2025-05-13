use serde::{Deserialize, Serialize};
use stepflow_schema::ObjectSchema;
use stepflow_workflow::Component;

use crate::Method;

/// Request from the runtime for information about a specific component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub component: Component,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// The input schema for the component.
    ///
    /// This should be a JSON object.
    pub input_schema: ObjectSchema,

    /// The output schema for the component.
    ///
    /// This should be a JSON object.
    pub output_schema: ObjectSchema,

    /// Whether the component should be executed even if it has no inputs.
    #[serde(default)]
    pub always_execute: bool,
}

/// Response to the initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Information about the component.
    #[serde(flatten)]
    pub info: ComponentInfo,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "component_info";
    type Response = Response;
}
