use serde::{Deserialize, Serialize};
use stepflow_core::workflow::{Component, ValueRef};

use crate::schema::Method;

/// Request from the runtime to initialize the component server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub component: Component,
    pub input: ValueRef,
}

/// Response to the initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub output: ValueRef,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "component_execute";

    type Response = Response;
}
