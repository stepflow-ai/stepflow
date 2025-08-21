// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

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

use std::borrow::Cow;

use ::serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use uuid::Uuid;

use crate::lazy_value::LazyValue;
use crate::protocol::json_rpc::JsonRpc;
use crate::protocol::{Method, method_params, method_result, notification_params};

#[derive(Serialize, Debug, JsonSchema)]
#[serde(untagged)]
/// The messages supported by the Stepflow protocol. These correspond to JSON-RPC 2.0 messages.
///
/// Note that this defines a superset containing both client-sent and server-sent messages.
#[schemars(schema_with = "message_schema")]
pub enum Message<'a> {
    Request(MethodRequest<'a>),
    Response(MethodResponse<'a>),
    Notification(Notification<'a>),
}

fn message_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    // We want to flatten the two cases for `MethodResponse`, while still generating
    // the schema for method response.

    let request = generator.subschema_for::<MethodRequest<'_>>();

    // Generate the MethodResponse schema even though we don't need it for messages.
    // This ensures a type is generated to represent method responses.
    let _response = generator.subschema_for::<MethodResponse<'_>>();
    let success = generator.subschema_for::<MethodSuccess<'_>>();
    let error = generator.subschema_for::<MethodError<'_>>();
    let notifification = generator.subschema_for::<Notification<'_>>();

    schemars::json_schema!({
        "oneOf": [
            request, success, error, notifification
        ]
    })
}

impl<'a> Message<'a> {
    pub fn into_response(self) -> Option<MethodResponse<'a>> {
        match self {
            Self::Response(r) => Some(r),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Eq, PartialEq, Clone, Hash)]
#[serde(untagged)]
/// The identifier for a JSON-RPC request. Can be either a string or an integer.
/// The RequestId is used to match method responses to corresponding requests.
/// It should not be set on notifications.
pub enum RequestId {
    String(String),
    Integer(i64),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::String(s) => write!(f, "{s}"),
            RequestId::Integer(i) => write!(f, "{i}"),
        }
    }
}

impl RequestId {
    /// Generate a new RequestId using a UUID string.
    pub fn new_uuid() -> Self {
        RequestId::from(uuid::Uuid::new_v4())
    }
}

impl From<i64> for RequestId {
    fn from(i: i64) -> Self {
        RequestId::Integer(i)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

impl From<&'static str> for RequestId {
    fn from(s: &'static str) -> Self {
        RequestId::String(s.to_string())
    }
}

impl From<Uuid> for RequestId {
    fn from(uuid: Uuid) -> Self {
        RequestId::String(uuid.to_string())
    }
}

impl From<&'_ Uuid> for RequestId {
    fn from(uuid: &'_ Uuid) -> Self {
        RequestId::String(uuid.to_string())
    }
}

/// An error returned from a method execution.
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Error<'a> {
    /// A numeric code indicating the error type.
    pub code: i64,
    /// Concise, single-sentence description of the error.
    pub message: Cow<'a, str>,
    /// Primitive or structured value that contains additional information about the error.
    #[serde(borrow)]
    pub data: Option<LazyValue<'a>>,
}

impl<'a> Error<'a> {
    pub fn method_not_found(method: Method) -> Self {
        Error {
            code: -32601, // Method not found
            message: Cow::Owned(format!("Method not found: '{method}'")),
            data: None,
        }
    }

    pub fn invalid_parameters(id: &RequestId) -> Self {
        Error {
            code: -32602, // Invalid params
            message: Cow::Owned(format!("Invalid parameters for request {id}")),
            data: None,
        }
    }

    pub fn internal(message: impl Into<Cow<'a, str>>) -> Self {
        Error {
            code: -32000, // Internal error
            message: message.into(),
            data: None,
        }
    }

    pub fn invalid_value(field: &str, expected: &str) -> Self {
        Error {
            code: -32012,
            message: format!("Invalid value for field '{field}': expected {expected}").into(),
            data: None,
        }
    }

    pub fn not_found(entity: &str, id: &str) -> Self {
        Error {
            code: -32011,
            message: format!("Not found: {entity} with ID '{id}'").into(),
            data: None,
        }
    }
}

/// Request to execute a method.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MethodRequest<'a> {
    #[serde(default)]
    pub jsonrpc: JsonRpc,
    pub id: RequestId,
    /// The method being called.
    pub method: Method,
    /// The parameters for the method call. Set on method requests.
    #[schemars(schema_with = "method_params")]
    pub params: Option<LazyValue<'a>>,
}

impl<'a> MethodRequest<'a> {
    pub fn new(id: impl Into<RequestId>, method: Method, params: Option<LazyValue<'a>>) -> Self {
        MethodRequest {
            jsonrpc: JsonRpc,
            id: id.into(),
            method,
            params,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// The result of a successful method execution.
pub struct MethodSuccess<'a> {
    #[serde(default)]
    pub jsonrpc: JsonRpc,
    pub id: RequestId,
    /// The result of a successful method execution.
    #[serde(borrow)]
    #[schemars(schema_with = "method_result")]
    pub result: LazyValue<'a>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MethodError<'a> {
    #[serde(default)]
    pub jsonrpc: JsonRpc,
    pub id: RequestId,
    /// An error that occurred during method execution.
    #[serde(borrow)]
    pub error: Error<'a>,
}

/// Response to a method request.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum MethodResponse<'a> {
    Success(MethodSuccess<'a>),
    Error(MethodError<'a>),
}

impl<'a> MethodResponse<'a> {
    pub fn success(id: impl Into<RequestId>, result: LazyValue<'a>) -> Self {
        MethodResponse::Success(MethodSuccess {
            jsonrpc: JsonRpc,
            id: id.into(),
            result,
        })
    }

    pub fn error(id: impl Into<RequestId>, error: Error<'a>) -> Self {
        MethodResponse::Error(MethodError {
            jsonrpc: JsonRpc,
            id: id.into(),
            error,
        })
    }

    pub fn id(&self) -> &RequestId {
        match self {
            MethodResponse::Success(s) => &s.id,
            MethodResponse::Error(e) => &e.id,
        }
    }

    pub fn into_success(self) -> Option<LazyValue<'a>> {
        match self {
            MethodResponse::Success(s) => Some(s.result),
            _ => None,
        }
    }
}

/// Notification.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Notification<'a> {
    #[serde(default)]
    pub jsonrpc: JsonRpc,
    /// The notification method being called.
    pub method: Method,
    /// The parameters for the notification.
    #[serde(borrow)]
    #[schemars(schema_with = "notification_params")]
    pub params: Option<LazyValue<'a>>,
}

impl<'a> Notification<'a> {
    pub fn new(method: Method, params: Option<LazyValue<'a>>) -> Self {
        Notification {
            jsonrpc: JsonRpc,
            method,
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use similar_asserts::assert_eq;
    use std::env;

    #[test]
    fn test_request_id_variants() {
        let string_id = RequestId::String("test".into());
        let int_id = RequestId::Integer(42);

        assert_eq!(serde_json::to_string(&string_id).unwrap(), r#""test""#);
        assert_eq!(serde_json::to_string(&int_id).unwrap(), "42");

        let parsed_string: RequestId = serde_json::from_str(r#""test""#).unwrap();
        let parsed_int: RequestId = serde_json::from_str("42").unwrap();

        assert_eq!(parsed_string, string_id);
        assert_eq!(parsed_int, int_id);
    }

    #[test]
    fn test_error_object() {
        let error_data = json!({"param": "missing"});
        let error = Error {
            code: -32602,
            message: "Invalid params".into(),
            data: Some(LazyValue::write_ref(&error_data)),
        };

        let serialized = serde_json::to_string(&error).expect("Failed to serialize error");
        let parsed: Error<'_> =
            serde_json::from_str(&serialized).expect("Failed to deserialize error");

        assert_eq!(parsed.code, -32602);
        assert_eq!(parsed.message.as_ref(), "Invalid params");
        assert!(parsed.data.is_some());
    }

    /// Helper function to validate that all titles in a schema are valid Python class names
    fn validate_python_class_names(schema: &serde_json::Value) -> Vec<String> {
        let mut invalid_titles = Vec::new();

        fn is_valid_python_class_name(name: &str) -> bool {
            // Python class names must:
            // - Start with a letter or underscore
            // - Contain only letters, numbers, and underscores
            // - Not be a Python keyword

            // Check basic pattern
            if !name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            {
                return false;
            }

            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return false;
            }

            // Check if it's not a Python keyword
            let python_keywords = [
                "False", "None", "True", "and", "as", "assert", "break", "class", "continue",
                "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
                "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
                "try", "while", "with", "yield", "async", "await",
            ];

            !python_keywords.contains(&name)
        }

        fn extract_titles_recursive(obj: &serde_json::Value, invalid_titles: &mut Vec<String>) {
            match obj {
                serde_json::Value::Object(map) => {
                    // Check if this object has a title
                    if let Some(serde_json::Value::String(title)) = map.get("title") {
                        if !is_valid_python_class_name(title) {
                            invalid_titles.push(title.clone());
                        }
                    }

                    // Recursively search in all values
                    for value in map.values() {
                        extract_titles_recursive(value, invalid_titles);
                    }
                }
                serde_json::Value::Array(arr) => {
                    // Recursively search in all array items
                    for item in arr {
                        extract_titles_recursive(item, invalid_titles);
                    }
                }
                _ => {}
            }
        }

        extract_titles_recursive(schema, &mut invalid_titles);
        invalid_titles
    }

    #[test]
    fn test_schema_comparison_with_protocol_json() {
        let mut generator = schemars::generate::SchemaSettings::draft2020_12().into_generator();
        let generated_schema = generator.root_schema_for::<Message<'static>>();
        let generated_json = serde_json::to_value(&generated_schema)
            .expect("Failed to convert generated schema to JSON");

        // Validate that all titles are valid Python class names
        let invalid_titles = validate_python_class_names(&generated_json);
        assert!(
            invalid_titles.is_empty(),
            "Found invalid Python class names in protocol schema titles: {invalid_titles:?}. \
             All titles must be valid Python class names for --use-title-as-name to work."
        );

        // Read the reference schema
        let protocol_schema_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../schemas/protocol.json"
        );

        // Check if we should overwrite the reference schema or if it doesn't exist
        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            // Ensure the directory exists
            if let Some(parent) = std::path::Path::new(protocol_schema_path).parent() {
                std::fs::create_dir_all(parent).expect("Failed to create schema directory");
            }

            std::fs::write(
                protocol_schema_path,
                serde_json::to_string_pretty(&generated_json).unwrap(),
            )
            .expect("Failed to write updated schema");
        } else {
            // Try to read the existing schema for comparison
            match std::fs::read_to_string(protocol_schema_path) {
                Ok(protocol_schema_str) => {
                    let protocol_schema: serde_json::Value =
                        serde_json::from_str(&protocol_schema_str)
                            .expect("Failed to parse protocol schema JSON");

                    // Normalize both schemas to pretty-printed JSON strings for comparison
                    let generated_schema_str = serde_json::to_string_pretty(&generated_json)
                        .expect("Failed to serialize generated schema");
                    let expected_schema_str = serde_json::to_string_pretty(&protocol_schema)
                        .expect("Failed to serialize expected schema");

                    // Use similar_asserts for better diff output when schemas don't match
                    assert_eq!(
                        generated_schema_str, expected_schema_str,
                        "Generated schema does not match the reference schema at {}. \
                         Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-protocol' to update.",
                        protocol_schema_path
                    );
                }
                Err(_) => {
                    // File doesn't exist, fail the test with helpful message
                    panic!(
                        "Protocol schema file not found at {protocol_schema_path}. Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-protocol' to create it."
                    );
                }
            }
        }
    }
}
