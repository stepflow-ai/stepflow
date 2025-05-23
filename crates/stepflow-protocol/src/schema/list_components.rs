use serde::{Deserialize, Serialize};
use stepflow_core::workflow::Component;

use crate::schema::Method;

/// Request from the runtime to initialize the component server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {}

/// Response to the initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub components: Vec<Component>,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "list_components";
    type Response = Response;
}
