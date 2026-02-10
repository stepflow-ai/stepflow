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

use serde::{Deserialize, Serialize};
use strum::IntoStaticStr;
use utoipa::ToSchema;

#[derive(
    Serialize, Deserialize, Debug, ToSchema, Hash, PartialEq, Eq, Clone, Copy, IntoStaticStr,
)]
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
    #[serde(rename = "components/infer_schema")]
    ComponentsInferSchema,
    #[serde(rename = "runs/submit")]
    RunsSubmit,
    #[serde(rename = "runs/get")]
    RunsGet,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Initialize => write!(f, "initialize"),
            Method::Initialized => write!(f, "initialized"),
            Method::ComponentsList => write!(f, "components/list"),
            Method::ComponentsInfo => write!(f, "components/info"),
            Method::ComponentsExecute => write!(f, "components/execute"),
            Method::ComponentsInferSchema => write!(f, "components/infer_schema"),
            Method::RunsSubmit => write!(f, "runs/submit"),
            Method::RunsGet => write!(f, "runs/get"),
        }
    }
}

pub trait ProtocolMethod {
    const METHOD_NAME: Method;
    type Response: Sync + Send + std::fmt::Debug;

    fn observability_context(&self) -> Option<&crate::protocol::ObservabilityContext> {
        None
    }
}

pub trait ProtocolNotification {
    const METHOD_NAME: Method;
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
        assert_eq!(
            serde_json::to_string(&Method::RunsSubmit).unwrap(),
            r#""runs/submit""#
        );
        assert_eq!(
            serde_json::to_string(&Method::RunsGet).unwrap(),
            r#""runs/get""#
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
        assert_eq!(
            serde_json::from_str::<Method>(r#""runs/submit""#).unwrap(),
            Method::RunsSubmit
        );
        assert_eq!(
            serde_json::from_str::<Method>(r#""runs/get""#).unwrap(),
            Method::RunsGet
        );
    }

    #[test]
    #[should_panic]
    fn test_method_unknown_deserialization() {
        // Should fail for unknown method names
        serde_json::from_str::<Method>(r#""components/unknown""#).unwrap();
    }
}
