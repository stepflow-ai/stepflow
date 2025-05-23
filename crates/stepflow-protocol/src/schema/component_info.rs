use serde::{Deserialize, Serialize};
use stepflow_core::{component::ComponentInfo, workflow::Component};

use crate::schema::Method;

/// Request from the runtime for information about a specific component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub component: Component,
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
