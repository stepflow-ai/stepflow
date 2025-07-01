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

use schemars::{JsonSchema, Schema, json_schema};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, Hash, PartialEq, Eq, Clone, Copy)]
pub enum Method {
    #[serde(rename = "initialize")]
    Initialize,
    #[serde(rename = "initialized")]
    Initialized,
    #[serde(rename = "components/list")]
    ComponentsList,
    #[serde(rename = "components/info")]
    ComponentsInfo,
    #[serde(rename = "components/execute")]
    ComponentsExecute,
    #[serde(rename = "blobs/put")]
    BlobsPut,
    #[serde(rename = "blobs/get")]
    BlobsGet,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Initialize => write!(f, "initialize"),
            Method::Initialized => write!(f, "initialized"),
            Method::ComponentsList => write!(f, "components/list"),
            Method::ComponentsInfo => write!(f, "components/info"),
            Method::ComponentsExecute => write!(f, "components/execute"),
            Method::BlobsPut => write!(f, "blobs/put"),
            Method::BlobsGet => write!(f, "blobs/get"),
        }
    }
}

pub(crate) trait ProtocolMethod {
    const METHOD_NAME: Method;
    type Response: Sync + Send + std::fmt::Debug;
}

pub(crate) trait ProtocolNotification {
    const METHOD_NAME: Method;
}

pub(crate) fn method_params(generator: &mut schemars::SchemaGenerator) -> Schema {
    let params = vec![
        generator.subschema_for::<super::initialization::InitializeParams>(),
        generator.subschema_for::<super::components::ComponentExecuteParams>(),
        generator.subschema_for::<super::components::ComponentInfoParams>(),
        generator.subschema_for::<super::components::ComponentListParams>(),
        generator.subschema_for::<super::blobs::GetBlobParams>(),
        generator.subschema_for::<super::blobs::PutBlobParams>(),
    ];
    json_schema!({
        "title": "Method Parameters",
        "description": "Parameters for the method call.",
        "oneOf": params
    })
}

pub(crate) fn method_result(generator: &mut schemars::SchemaGenerator) -> Schema {
    let params: Vec<Schema> = vec![
        generator.subschema_for::<super::initialization::InitializeResult>(),
        generator.subschema_for::<super::components::ComponentExecuteResult>(),
        generator.subschema_for::<super::components::ComponentInfoResult>(),
        generator.subschema_for::<super::components::ListComponentsResult>(),
        generator.subschema_for::<super::blobs::GetBlobResult>(),
        generator.subschema_for::<super::blobs::PutBlobResult>(),
    ];
    json_schema!({
        "title": "Method Result",
        "description": "Result of the method call.",
        "oneOf": params
    })
}

pub(crate) fn notification_params(generator: &mut schemars::SchemaGenerator) -> Schema {
    let params = vec![generator.subschema_for::<super::initialization::Initialized>()];
    json_schema!({
        "title": "Notification Parameters",
        "description": "Parameters for the notification.",
        "oneOf": params
    })
}

#[cfg(test)]
mod tests {
    use super::Method;

    #[test]
    fn test_method_serialization() {
        assert_eq!(
            serde_json::to_string(&Method::Initialize).unwrap(),
            r#""initialize""#
        );
        assert_eq!(
            serde_json::to_string(&Method::ComponentsList).unwrap(),
            r#""components/list""#
        );
    }

    #[test]
    fn test_method_deserialization() {
        assert_eq!(
            serde_json::from_str::<Method>(r#""initialize""#).unwrap(),
            Method::Initialize
        );
        assert_eq!(
            serde_json::from_str::<Method>(r#""components/list""#).unwrap(),
            Method::ComponentsList
        );
    }

    #[test]
    #[should_panic]
    fn test_method_unknown_deserialization() {
        // Should fail for unknown method names
        serde_json::from_str::<Method>(r#""components/unknown""#).unwrap();
    }
}
