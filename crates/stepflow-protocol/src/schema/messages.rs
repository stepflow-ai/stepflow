use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use uuid::Uuid;

/// A method in the StepFlow protocol.
///
/// The request type should implement this trait.
pub trait Method {
    /// The name of the method.
    const METHOD_NAME: &'static str;
    /// The expected response type.
    type Response;
}

/// Notifications from the component server to the runtime.
pub trait Notification {
    const NOTIFICATION_NAME: &'static str;
}

/// Message sent to request a method execution.
#[derive(Serialize, Deserialize)]
pub struct RequestMessage<'a, I> {
    /// The JSON-RPC version (must be "2.0")
    #[serde(borrow)]
    pub jsonrpc: &'a str,
    /// The request id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// The method to execute.
    #[serde(borrow)]
    pub method: &'a str,
    /// The parameters to pass to the method.
    pub params: I,
}

/// Message sent in response to a method request.
///
/// Exactly one of `result` or `error` should be present.
#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseMessage<'a> {
    /// The JSON-RPC version (must be "2.0")
    pub jsonrpc: &'a str,
    /// The request id.
    pub id: Uuid,
    /// The result of the method execution.
    pub result: Option<&'a RawValue>,
    /// The error that occurred during the method execution.
    pub error: Option<RemoteError<'a>>,
}

/// An error returned from a method execution.
#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteError<'a> {
    pub code: i64,
    pub message: &'a str,
    pub data: Option<&'a RawValue>,
}

#[derive(Serialize, Deserialize)]
pub struct NotificationMessage<'a, I> {
    pub jsonrpc: &'a str,
    pub method: &'a str,
    pub params: I,
}
