use std::sync::Arc;

use stepflow_execution::StepFlowExecutor;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod components;
mod debug;
mod flows;
mod health;
mod runs;

const COMPONENT_TAG: &str = "Component";
const FLOW_TAG: &str = "Flow";
const RUN_TAG: &str = "Run";
const DEBUG_TAG: &str = "Debug";

pub use runs::{CreateRunRequest, CreateRunResponse};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "StepFlow API",
        description = "API for StepFlow workflows and executions",
        version = env!("CARGO_PKG_VERSION")
    ),
    tags(
        (name = COMPONENT_TAG, description = "Component API endpoints"),
        (name = FLOW_TAG, description = "Flow API endpoints"),
        (name = RUN_TAG, description = "Run API endpoints"),
        (name = DEBUG_TAG, description = "Debug API endpoints")
    ),
    paths(
        health::health_check,
        components::list_components,
        debug::debug_execute_step,
        debug::debug_continue,
        debug::debug_get_runnable,
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
        flows::get_flow_dependencies,
    ),
    components(schemas(
        components::ComponentInfoResponse,
        components::ListComponentsResponse,
        components::ListComponentsQuery,
        debug::DebugStepRequest,
        debug::DebugStepResponse,
        debug::DebugRunnableResponse,
        health::HealthResponse,
        runs::CreateRunRequest,
        runs::CreateRunResponse,
        runs::RunSummaryResponse,
        runs::RunDetailsResponse,
        runs::ListRunsResponse,
        runs::StepRunResponse,
        runs::ListStepRunsResponse,
        runs::RunFlowResponse,
        flows::StoreFlowRequest,
        flows::StoreFlowResponse,
        flows::FlowResponse,
        flows::FlowDependenciesResponse,
    )),
)]
struct StepflowApi;

pub fn create_api_router() -> OpenApiRouter<Arc<StepFlowExecutor>> {
    OpenApiRouter::with_openapi(StepflowApi::openapi())
        .routes(routes!(health::health_check))
        .routes(routes!(components::list_components))
        .routes(routes!(debug::debug_execute_step))
        .routes(routes!(debug::debug_continue))
        .routes(routes!(debug::debug_get_runnable))
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
        .routes(routes!(flows::get_flow_dependencies))
}
