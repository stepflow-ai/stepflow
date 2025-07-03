// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use crate::LazyValue;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer};

use super::json_rpc::JsonRpc;
use super::{Error, Message, Method, MethodRequest, MethodResponse, Notification, RequestId};

/// Intermediate message type for custom deserialization
#[derive(Debug, Deserialize)]
struct RawMessage<'a> {
    jsonrpc: JsonRpc,
    id: Option<RequestId>,
    #[serde(default)]
    method: Option<Method>,
    #[serde(borrow, default)]
    params: Option<LazyValue<'a>>,
    #[serde(borrow, default)]
    result: Option<LazyValue<'a>>,
    #[serde(borrow, default)]
    error: Option<Error<'a>>,
}

struct MessageVisitor<'de> {
    _phantom: std::marker::PhantomData<&'de ()>,
}

impl<'de> MessageVisitor<'de> {
    fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<'de> Visitor<'de> for MessageVisitor<'de> {
    type Value = Message<'de>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON-RPC 2.0 message (request, response, notification, or batch)")
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        use serde::de::Error as _;

        // Deserialize as RawMessage to inspect the fields
        let raw_message =
            RawMessage::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
        let RawMessage {
            jsonrpc,
            id,
            method,
            params,
            result,
            error,
        } = raw_message;

        // Determine message type based on fields present
        match (id, method) {
            (Some(id), Some(method)) => {
                if result.is_some() {
                    return Err(A::Error::custom("unexpected 'result' with 'method'"));
                }
                if error.is_some() {
                    return Err(A::Error::custom("unexpected 'error' with 'method'"));
                }
                Ok(Message::Request(MethodRequest {
                    jsonrpc,
                    id,
                    method,
                    params,
                }))
            }
            (None, Some(method)) => {
                if result.is_some() {
                    return Err(A::Error::custom("unexpected 'result' with 'method'"));
                }
                if error.is_some() {
                    return Err(A::Error::custom("unexpected 'error' with 'method'"));
                }
                Ok(Message::Notification(Notification {
                    jsonrpc,
                    method,
                    params,
                }))
            }
            (Some(id), None) => {
                let response = match (result, error) {
                    (None, None) => {
                        return Err(A::Error::custom("missing 'result' or 'error' in response"));
                    }
                    (Some(result), None) => MethodResponse::success(id, result),
                    (None, Some(error)) => MethodResponse::error(id, error),
                    (Some(_), Some(_)) => {
                        return Err(A::Error::custom(
                            "both 'result' and 'error' present in response",
                        ));
                    }
                };

                Ok(Message::Response(response))
            }
            (None, None) => Err(A::Error::custom("missing 'id' or 'method'")),
        }
    }
}

impl<'de> Deserialize<'de> for Message<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(MessageVisitor::new())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Error, MethodResponse};

    use super::*;
    use serde_json::json;
    use similar_asserts::assert_eq;

    // Individual type serialization/deserialization tests

    #[test]
    fn test_request_deserialization() {
        let json_str =
            r#"{"jsonrpc":"2.0","id":"test-id","method":"initialize","params":{"key":"value"}}"#;

        let request: Message<'_> =
            serde_json::from_str(json_str).expect("Failed to deserialize request");
        let Message::Request(request) = request else {
            panic!("Expected request message, got {request:?}");
        };
        assert_eq!(request.method, Method::Initialize);
        assert_eq!(request.id, RequestId::String("test-id".to_string()));
        assert!(request.params.is_some());
    }

    #[test]
    fn test_request_serialization() {
        let params = json!({"key": "value"});
        let request = MethodRequest::new(
            "test-id",
            Method::Initialize,
            Some(LazyValue::write_ref(&params)),
        );

        let serialized = serde_json::to_string(&request).expect("Failed to serialize request");
        let expected =
            r#"{"jsonrpc":"2.0","id":"test-id","method":"initialize","params":{"key":"value"}}"#;

        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_response_success_deserialization() {
        let response_json = r#"{"jsonrpc":"2.0","id":123,"result":{"success":true}}"#;
        let response: Message<'_> = serde_json::from_str(response_json).unwrap();

        let Message::Response(response) = response else {
            panic!("Expected response message");
        };

        assert_eq!(response.id(), &RequestId::Integer(123));

        match &response {
            MethodResponse::Success(success) => {
                // LazyValue from deserialization should be Read variant, so deserialize_to should work
                let value: serde_json::Value = success.result.deserialize_to().unwrap();
                assert_eq!(value["success"], true);
            }
            _ => panic!("Expected result response"),
        }
    }

    #[test]
    fn test_response_success_serialization() {
        let result_data = json!({"success": true});
        let response = MethodResponse::success(123, LazyValue::write_ref(&result_data));

        let serialized = serde_json::to_string(&response).expect("Failed to serialize response");
        let expected = r#"{"jsonrpc":"2.0","id":123,"result":{"success":true}}"#;

        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_response_error_deserialization() {
        let error_json = r#"{"jsonrpc":"2.0","id":123,"error":{"code":-32600,"message":"Invalid Request","data":null}}"#;
        let response: Message<'_> = serde_json::from_str(error_json).unwrap();
        let Message::Response(response) = response else {
            panic!("Expected response message, got {response:?}");
        };

        match &response {
            MethodResponse::Error(error) => {
                let error = &error.error;
                assert_eq!(error.code, -32600);
                assert_eq!(error.message.as_ref(), "Invalid Request");
                assert!(error.data.is_none());
            }
            _ => panic!("Expected error response"),
        }
    }

    #[test]
    fn test_response_error_serialization() {
        let error = Error {
            code: -32600,
            message: "Invalid Request".into(),
            data: None,
        };

        let response = MethodResponse::error(123, error);

        let serialized = serde_json::to_string(&response).expect("Failed to serialize response");
        let expected = r#"{"jsonrpc":"2.0","id":123,"error":{"code":-32600,"message":"Invalid Request","data":null}}"#;

        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_notification_deserialization() {
        let notification_json =
            r#"{"jsonrpc":"2.0","method":"initialize","params":{"data":"test"}}"#;
        let notification: Notification<'_> = serde_json::from_str(notification_json).unwrap();

        assert_eq!(notification.method, Method::Initialize);
        assert!(notification.params.is_some());
    }

    #[test]
    fn test_notification_serialization() {
        let params = json!({"data": "test"});
        let notification =
            Notification::new(Method::Initialize, Some(LazyValue::write_ref(&params)));

        let serialized =
            serde_json::to_string(&notification).expect("Failed to serialize notification");
        let expected = r#"{"jsonrpc":"2.0","method":"initialize","params":{"data":"test"}}"#;

        assert_eq!(serialized, expected);
    }

    // Message enum deserialization tests (testing the Message enum's ability to parse into correct variants)

    #[test]
    fn test_message_request_deserialization() {
        let request_json = r#"{"jsonrpc":"2.0","id":"test","method":"components/execute","params":{"minuend":42,"subtrahend":23}}"#;
        let message: Message<'_> = serde_json::from_str(request_json).unwrap();

        match message {
            Message::Request(req) => {
                assert_eq!(req.method, Method::ComponentsExecute);
                assert_eq!(req.id, RequestId::String("test".to_string()));
            }
            _ => panic!("Expected request message"),
        }
    }

    #[test]
    fn test_message_notification_deserialization() {
        let notification_json = r#"{"jsonrpc":"2.0","method":"initialize","params":[1,2,3,4,5]}"#;
        let message: Message<'_> = serde_json::from_str(notification_json).unwrap();

        match message {
            Message::Notification(notif) => {
                assert_eq!(notif.method, Method::Initialize);
                assert!(notif.params.is_some());
            }
            _ => panic!("Expected notification message"),
        }
    }

    #[test]
    fn test_message_response_success_primitive_deserialization() {
        let response_json = r#"{"jsonrpc":"2.0","id":1,"result":19}"#;
        let message: Message<'_> = serde_json::from_str(response_json).unwrap();

        match message {
            Message::Response(resp) => {
                assert_eq!(resp.id(), &RequestId::Integer(1));
                match &resp {
                    MethodResponse::Success(success) => {
                        let result: serde_json::Value = success.result.deserialize_to().unwrap();
                        assert_eq!(result.as_i64().unwrap(), 19);
                    }
                    _ => panic!("Expected result response"),
                }
            }
            _ => panic!("Expected response message"),
        }
    }

    #[test]
    fn test_message_response_error_deserialization() {
        let error_json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let message: Message<'_> = serde_json::from_str(error_json).unwrap();

        match message {
            Message::Response(MethodResponse::Error(error)) => {
                let error = &error.error;
                assert_eq!(error.code, -32601);
                assert_eq!(error.message.as_ref(), "Method not found");
            }
            _ => panic!("Expected response message"),
        }
    }

    #[test]
    fn test_message_invalid_format() {
        let invalid_json = r#"{"jsonrpc":"2.0"}"#;
        let result: Result<Message<'_>, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing 'id' or 'method'")
        );
    }

    // JSON-RPC 2.0 spec compliance tests

    #[test]
    fn test_json_rpc_spec_compliance() {
        // Test JSON-RPC 2.0 spec examples

        // Request with positional parameters
        let request_json =
            r#"{"jsonrpc": "2.0", "method": "components/list", "params": [42, 23], "id": 1}"#;
        let message: Message<'_> = serde_json::from_str(request_json).unwrap();
        let Message::Request(request) = message else {
            panic!("Expected request message");
        };
        assert_eq!(request.method, Method::ComponentsList);

        // Request with named parameters
        let request_json = r#"{"jsonrpc": "2.0", "method": "components/info", "params": {"subtrahend": 23, "minuend": 42}, "id": 3}"#;
        let message: Message<'_> = serde_json::from_str(request_json).unwrap();
        let Message::Request(request) = message else {
            panic!("Expected request message");
        };
        assert_eq!(request.method, Method::ComponentsInfo);

        // Notification
        let notification_json =
            r#"{"jsonrpc": "2.0", "method": "initialize", "params": [1,2,3,4,5]}"#;
        let message: Message<'_> = serde_json::from_str(notification_json).unwrap();
        let Message::Notification(notification) = message else {
            panic!("Expected notification message");
        };
        assert_eq!(notification.method, Method::Initialize);
    }
}
