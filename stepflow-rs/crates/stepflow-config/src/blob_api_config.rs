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

/// Configuration for the Blob HTTP API.
///
/// This controls whether the orchestrator serves blob API endpoints and what URL
/// workers should use to access the blob API.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlobApiConfig {
    /// Whether the orchestrator serves blob API endpoints.
    ///
    /// Set to `false` when running a separate blob service.
    /// Default: `true`
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// URL workers use to access the blob API.
    ///
    /// If not set, defaults to `http://localhost:{port}/api/v1` where `{port}`
    /// is the server's bound port.
    ///
    /// Examples:
    /// - Local dev: omit (auto-detected)
    /// - K8s with orchestrator blobs: `http://orchestrator-service/api/v1`
    /// - K8s with separate blob service: `http://blob-service/api/v1`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for BlobApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            url: None,
        }
    }
}
