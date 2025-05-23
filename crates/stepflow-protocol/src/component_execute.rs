use serde::{Deserialize, Serialize};
use stepflow_core::workflow::{Component, Value};

use crate::Method;

/// Request from the runtime to initialize the component server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub component: Component,
    pub input: Value,
}

/// Response to the initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub output: Value,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "component_execute";

    type Response = Response;
}
