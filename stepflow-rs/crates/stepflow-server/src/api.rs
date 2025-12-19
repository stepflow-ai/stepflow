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

use stepflow_execution::StepflowExecutor;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod batches;
mod components;
mod debug;
mod flows;
mod health;
mod runs;

const BATCH_TAG: &str = "Batch";
const COMPONENT_TAG: &str = "Component";
const FLOW_TAG: &str = "Flow";
const RUN_TAG: &str = "Run";
const DEBUG_TAG: &str = "Debug";

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
        (name = BATCH_TAG, description = "Batch API endpoints"),
        (name = COMPONENT_TAG, description = "Component API endpoints"),
        (name = FLOW_TAG, description = "Flow API endpoints"),
        (name = RUN_TAG, description = "Run API endpoints"),
        (name = DEBUG_TAG, description = "Debug API endpoints")
    ),
    paths(
        health::health_check,
        components::list_components,
        debug::debug_eval,
        debug::debug_queue,
        debug::debug_next,
        debug::debug_run_queue,
        debug::debug_get_queue,
        debug::debug_show,
        runs::create_run,
        runs::get_run,
        runs::get_run_flow,
        runs::list_runs,
        runs::get_run_steps,
        runs::cancel_run,
        runs::delete_run,
        flows::store_flow,
        flows::get_flow,
        flows::delete_flow,
        batches::create_batch,
        batches::get_batch,
        batches::list_batches,
        batches::list_batch_runs,
        batches::get_batch_outputs,
        batches::cancel_batch,
    ),
    components(schemas(
        batches::CreateBatchRequest,
        batches::CreateBatchResponse,
        batches::GetBatchResponse,
        batches::ListBatchesResponse,
        batches::ListBatchesQuery,
        batches::BatchRunInfo,
        batches::ListBatchRunsResponse,
        batches::BatchOutputInfo,
        batches::ListBatchOutputsResponse,
        batches::CancelBatchResponse,
        stepflow_state::BatchMetadata,
        stepflow_state::BatchStatistics,
        stepflow_state::BatchDetails,
        stepflow_state::BatchStatus,
        components::ListComponentsResponse,
        components::ListComponentsQuery,
        debug::DebugEvalRequest,
        debug::DebugQueueRequest,
        debug::DebugEvalResponse,
        debug::DebugQueueResponse,
        debug::DebugNextResponse,
        debug::DebugRunQueueResponse,
        debug::DebugQueueStatusResponse,
        debug::QueuedStep,
        debug::DebugShowResponse,
        health::HealthQuery,
        health::HealthResponse,
        runs::CreateRunRequest,
        runs::CreateRunResponse,
        runs::ListRunsResponse,
        stepflow_state::RunSummary,
        stepflow_state::RunDetails,
        runs::StepRunResponse,
        runs::ListStepRunsResponse,
        runs::RunFlowResponse,
        flows::StoreFlowRequest,
        flows::StoreFlowResponse,
        flows::FlowResponse,
        stepflow_analysis::Diagnostic,
        stepflow_analysis::DiagnosticLevel,
        stepflow_analysis::DiagnosticMessage,
        stepflow_analysis::Diagnostics,
        stepflow_core::workflow::WorkflowOverrides,
        stepflow_core::workflow::StepOverride,
        stepflow_core::workflow::OverrideType,
    )),
)]
struct StepflowApi;

pub fn create_api_router() -> OpenApiRouter<Arc<StepflowExecutor>> {
    OpenApiRouter::with_openapi(StepflowApi::openapi())
        .routes(routes!(health::health_check))
        .routes(routes!(components::list_components))
        .routes(routes!(debug::debug_eval))
        .routes(routes!(debug::debug_queue))
        .routes(routes!(debug::debug_next))
        .routes(routes!(debug::debug_run_queue))
        .routes(routes!(debug::debug_get_queue))
        .routes(routes!(debug::debug_show))
        .routes(routes!(runs::create_run))
        .routes(routes!(runs::get_run))
        .routes(routes!(runs::get_run_flow))
        .routes(routes!(runs::list_runs))
        .routes(routes!(runs::get_run_steps))
        .routes(routes!(runs::cancel_run))
        .routes(routes!(runs::delete_run))
        .routes(routes!(flows::store_flow))
        .routes(routes!(flows::get_flow))
        .routes(routes!(flows::delete_flow))
        .routes(routes!(batches::create_batch))
        .routes(routes!(batches::get_batch))
        .routes(routes!(batches::list_batches))
        .routes(routes!(batches::list_batch_runs))
        .routes(routes!(batches::get_batch_outputs))
        .routes(routes!(batches::cancel_batch))
}
