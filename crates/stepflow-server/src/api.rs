use std::sync::Arc;

use stepflow_execution::StepFlowExecutor;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

mod components;
mod debug;
mod executions;
mod health;
mod workflows;

const COMPONENT_TAG: &str = "Component";
const WORKFLOW_TAG: &str = "Workflow";
const EXECUTION_TAG: &str = "Execution";
const DEBUG_TAG: &str = "Debug";

pub use executions::{CreateExecutionRequest, CreateExecutionResponse};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "StepFlow API",
        description = "API for StepFlow workflows and executions",
        version = env!("CARGO_PKG_VERSION")
    ),
    tags(
        (name = COMPONENT_TAG, description = "Component API endpoints"),
        (name = WORKFLOW_TAG, description = "Workflow API endpoints"),
        (name = EXECUTION_TAG, description = "Execution API endpoints"),
        (name = DEBUG_TAG, description = "Debug API endpoints")
    ),
    paths(
        health::health_check,
        components::list_components,
        debug::debug_execute_step,
        debug::debug_continue,
        debug::debug_get_runnable,
        executions::create_execution,
        executions::get_execution,
        executions::get_execution_workflow,
        executions::list_executions,
        executions::get_execution_steps,
        workflows::store_workflow,
        workflows::get_workflow,
        workflows::execute_workflow_by_hash,
        workflows::list_workflows,
        workflows::delete_workflow,
        workflows::get_workflow_dependencies,
        workflows::list_workflow_names,
        workflows::get_workflows_by_name,
        workflows::get_latest_workflow_by_name,
        workflows::execute_workflow_by_name,
        workflows::list_labels_for_name,
        workflows::create_or_update_label,
        workflows::get_workflow_by_label,
        workflows::execute_workflow_by_label,
        workflows::delete_label,
    ),
    components(schemas(
        components::ComponentInfoResponse,
        components::ListComponentsResponse,
        components::ListComponentsQuery,
        debug::DebugStepRequest,
        debug::DebugStepResponse,
        debug::DebugRunnableResponse,
        health::HealthResponse,
        executions::CreateExecutionRequest,
        executions::CreateExecutionResponse,
        executions::ExecutionSummaryResponse,
        executions::ExecutionDetailsResponse,
        executions::ListExecutionsResponse,
        executions::StepExecutionResponse,
        executions::ListStepExecutionsResponse,
        executions::ExecutionWorkflowResponse,
        workflows::StoreWorkflowRequest,
        workflows::StoreWorkflowResponse,
        workflows::WorkflowResponse,
        workflows::ListWorkflowsResponse,
        workflows::WorkflowDependenciesResponse,
        workflows::ListWorkflowNamesResponse,
        workflows::WorkflowNameQuery,
        workflows::WorkflowsByNameResponse,
        workflows::WorkflowVersionInfo,
        workflows::CreateLabelRequest,
        workflows::WorkflowLabelResponse,
        workflows::ListLabelsResponse,
        workflows::ExecuteWorkflowRequest,
        workflows::ExecuteWorkflowResponse
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
        .routes(routes!(executions::create_execution))
        .routes(routes!(executions::get_execution))
        .routes(routes!(executions::get_execution_workflow))
        .routes(routes!(executions::list_executions))
        .routes(routes!(executions::get_execution_steps))
        .routes(routes!(workflows::store_workflow))
        .routes(routes!(workflows::get_workflow))
        .routes(routes!(workflows::execute_workflow_by_hash))
        .routes(routes!(workflows::list_workflows))
        .routes(routes!(workflows::delete_workflow))
        .routes(routes!(workflows::get_workflow_dependencies))
        .routes(routes!(workflows::list_workflow_names))
        .routes(routes!(workflows::get_workflows_by_name))
        .routes(routes!(workflows::get_latest_workflow_by_name))
        .routes(routes!(workflows::execute_workflow_by_name))
        .routes(routes!(workflows::list_labels_for_name))
        .routes(routes!(workflows::create_or_update_label))
        .routes(routes!(workflows::get_workflow_by_label))
        .routes(routes!(workflows::execute_workflow_by_label))
        .routes(routes!(workflows::delete_label))
}
