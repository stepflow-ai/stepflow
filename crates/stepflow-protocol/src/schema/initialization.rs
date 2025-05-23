use serde::{Deserialize, Serialize};

use crate::schema::{Method, Notification};

/// Request from the runtime to initialize the component server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub runtime_protocol_version: u32,
}

/// Response to tho initializaiton request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub server_protocol_version: u32,
}

impl Method for Request {
    const METHOD_NAME: &'static str = "initialize";
    type Response = Response;
}

/// Notification from the runtime that initialization is complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Complete {}

impl Notification for Complete {
    const NOTIFICATION_NAME: &'static str = "initialized";
}
