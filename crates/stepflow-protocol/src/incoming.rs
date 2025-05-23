use crate::schema::RemoteError;
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use uuid::Uuid;

use crate::stdio::{Result, StdioError};

/// Message received from the child process.
///
/// Exactly one of `result` or `error` should be present.
#[derive(Serialize, Deserialize, Debug)]
pub struct Incoming<'a> {
    /// The JSON-RPC version (must be "2.0")
    pub jsonrpc: &'a str,
    /// The request id. If not set, this is a notification.
    pub id: Option<Uuid>,
    /// The result of the method execution.
    pub result: Option<&'a RawValue>,
    /// The error that occurred during the method execution.
    pub error: Option<RemoteError<'a>>,
}

/// Wrapper around a method response decoded from a string.
pub struct OwnedIncoming {
    // This is not used directly, but it is responsible for keeping
    // the JSON content that `message` references alive.
    #[allow(dead_code)]
    json: Box<str>,
    /// The incoming message.
    ///
    /// NOTE: This is not *really* `'static` but it needs to
    /// look like it is to appease the borrow checker. It is
    /// critical that this only be returned with a lifetime
    /// shorter than or equal to self (`'_`).
    message: Incoming<'static>,
}

impl std::fmt::Debug for OwnedIncoming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OwnedIncoming({:?})", self.message)
    }
}

impl OwnedIncoming {
    pub fn try_new(json: String) -> Result<Self> {
        let json = json.into_boxed_str();
        let message: Incoming<'_> =
            serde_json::from_str(&json).change_context(StdioError::InvalidMessage)?;

        // SAFETY: we take a reference to JSON, which we will ensure lives at least as long
        // as the message.
        let message = unsafe { std::mem::transmute::<Incoming<'_>, Incoming<'static>>(message) };
        Ok(Self { json, message })
    }

    pub fn raw_value(&self) -> Result<&'_ RawValue> {
        match (self.message.result, self.message.error.as_ref()) {
            (None, None) | (Some(_), Some(_)) => {
                error_stack::bail!(StdioError::InvalidResponse)
            }
            (Some(raw_result), None) => Ok(raw_result),
            (None, Some(error)) => {
                let data = if let Some(data) = error.data {
                    Some(
                        serde_json::from_str(data.get())
                            .change_context(StdioError::InvalidResponse)?,
                    )
                } else {
                    None
                };
                error_stack::bail!(StdioError::ServerError {
                    code: error.code,
                    message: error.message.to_owned(),
                    data,
                })
            }
        }
    }
}

impl std::ops::Deref for OwnedIncoming {
    type Target = Incoming<'static>;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_message(id: &Uuid, s: String) -> OwnedIncoming {
        OwnedIncoming::try_new(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": s,
            })
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn test_incoming_message() {
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        let message1 = create_message(&uuid1, "hello".to_string());
        let message2 = create_message(&uuid2, "world".to_string());

        assert_eq!(message1.id, Some(uuid1));
        assert_eq!(message2.id, Some(uuid2));
    }
}
