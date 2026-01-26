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

use std::sync::Arc;

use stepflow_plugin::StepflowEnvironment;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod components;
mod flows;
mod health;
mod runs;

const COMPONENT_TAG: &str = "Component";
const FLOW_TAG: &str = "Flow";
const RUN_TAG: &str = "Run";

pub use flows::{StoreFlowRequest, StoreFlowResponse};
pub use runs::{CreateRunRequest, CreateRunResponse};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Stepflow API",
        description = "API for Stepflow flows and runs",
        version = env!("CARGO_PKG_VERSION")
    ),
    tags(
        (name = COMPONENT_TAG, description = "Component API endpoints"),
        (name = FLOW_TAG, description = "Flow API endpoints"),
        (name = RUN_TAG, description = "Run API endpoints")
    ),
    paths(
        health::health_check,
        components::list_components,
        runs::create_run,
        runs::get_run,
        runs::get_run_items,
        runs::get_run_flow,
        runs::list_runs,
        runs::get_run_steps,
        runs::cancel_run,
        runs::delete_run,
        flows::store_flow,
        flows::get_flow,
        flows::delete_flow,
    ),
    components(schemas(
        components::ListComponentsResponse,
        components::ListComponentsQuery,
        health::HealthQuery,
        health::HealthResponse,
        runs::CreateRunRequest,
        runs::CreateRunResponse,
        runs::ListRunsResponse,
        runs::ListRunsQuery,
        stepflow_dtos::ItemResult,
        runs::ListItemsResponse,
        stepflow_dtos::RunSummary,
        stepflow_dtos::RunDetails,
        runs::StepRunResponse,
        runs::ListStepRunsResponse,
        runs::RunFlowResponse,
        flows::StoreFlowRequest,
        flows::StoreFlowResponse,
        flows::FlowResponse,
        stepflow_analysis::Diagnostic,
        stepflow_analysis::DiagnosticLevel,
        stepflow_analysis::Diagnostics,
        stepflow_core::workflow::WorkflowOverrides,
        stepflow_core::workflow::StepOverride,
        stepflow_core::workflow::OverrideType,
    )),
)]
struct StepflowApi;

pub fn create_api_router() -> OpenApiRouter<Arc<StepflowEnvironment>> {
    OpenApiRouter::with_openapi(StepflowApi::openapi())
        .routes(routes!(health::health_check))
        .routes(routes!(components::list_components))
        .routes(routes!(runs::create_run))
        .routes(routes!(runs::get_run))
        .routes(routes!(runs::get_run_items))
        .routes(routes!(runs::get_run_flow))
        .routes(routes!(runs::list_runs))
        .routes(routes!(runs::get_run_steps))
        .routes(routes!(runs::cancel_run))
        .routes(routes!(runs::delete_run))
        .routes(routes!(flows::store_flow))
        .routes(routes!(flows::get_flow))
        .routes(routes!(flows::delete_flow))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    /// This test generates the OpenAPI schema to schemas/openapi.json.
    /// Run with: STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-server test_openapi_schema_generation
    #[test]
    fn test_openapi_schema_generation() {
        let openapi = StepflowApi::openapi();
        let openapi_json =
            serde_json::to_string_pretty(&openapi).expect("Failed to serialize OpenAPI schema");

        // Get path to schemas/openapi.json
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let openapi_schema_path = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("schemas")
            .join("openapi.json");

        if env::var("STEPFLOW_OVERWRITE_SCHEMA").is_ok() {
            fs::write(&openapi_schema_path, &openapi_json).expect("Failed to write OpenAPI schema");
            println!(
                "Updated OpenAPI schema at {}",
                openapi_schema_path.display()
            );
        } else if openapi_schema_path.exists() {
            let existing = fs::read_to_string(&openapi_schema_path)
                .expect("Failed to read existing OpenAPI schema");

            // Parse both as JSON to compare (ignoring formatting differences)
            let existing_json: serde_json::Value =
                serde_json::from_str(&existing).expect("Failed to parse existing schema");
            let new_json: serde_json::Value =
                serde_json::from_str(&openapi_json).expect("Failed to parse new schema");

            assert_eq!(
                existing_json, new_json,
                "OpenAPI schema mismatch. Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-server test_openapi_schema_generation' to update."
            );
        } else {
            panic!(
                "OpenAPI schema file not found at {}. Run 'STEPFLOW_OVERWRITE_SCHEMA=1 cargo test -p stepflow-server test_openapi_schema_generation' to create it.",
                openapi_schema_path.display()
            );
        }
    }
}
