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

use tonic::{Request, Response, Status};

use crate::built_info;
use crate::proto::stepflow::v1::health_service_server::HealthService;
use crate::proto::stepflow::v1::{BuildInfo, HealthCheckRequest, HealthCheckResponse};

/// gRPC implementation of [HealthService].
#[derive(Debug, Default)]
pub struct HealthServiceImpl;

impl HealthServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

#[tonic::async_trait]
impl HealthService for HealthServiceImpl {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: "healthy".to_string(),
            timestamp: Some(crate::conversions::chrono_to_timestamp(chrono::Utc::now())),
            build: Some(build_info()),
        }))
    }
}

/// Construct [`BuildInfo`] from compile-time metadata provided by the `built` crate.
fn build_info() -> BuildInfo {
    BuildInfo {
        git_version: built_info::GIT_VERSION.unwrap_or("unknown").to_string(),
        dirty: built_info::GIT_DIRTY.unwrap_or(false),
        rustc_version: built_info::RUSTC_VERSION.to_string(),
        profile: built_info::PROFILE.to_string(),
        features: built_info::FEATURES.iter().map(|s| s.to_string()).collect(),
        target: built_info::TARGET.to_string(),
    }
}
