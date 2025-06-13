use std::sync::Arc;

use crate::Result;
use stepflow_execution::StepFlowExecutor;

/// Start the StepFlow HTTP server
///
/// # StepFlow Server API Documentation
///
/// The StepFlow server provides a comprehensive REST API for workflow execution and management.
/// The API follows RESTful principles and provides OpenAPI documentation.
///
/// ## Base URL
/// ```
/// http://localhost:7837/api/v1
/// ```
///
/// ## Core Features
///
/// ### 1. Ad-hoc Workflow Execution
/// Execute workflows directly without creating endpoints:
/// ```
/// POST /execute
/// ```
///
/// ### 2. Endpoint Management (with Optional Labels)
/// Manage named workflow endpoints with optional version labels:
/// ```
/// PUT    /endpoints/{name}?label={label}      # Create/update endpoint (label optional)
/// GET    /endpoints                           # List all endpoints
/// GET    /endpoints?name={name}               # List all versions of specific endpoint
/// GET    /endpoints/{name}?label={label}      # Get endpoint details (label optional)
/// GET    /endpoints/{name}/workflow?label={label}  # Get workflow definition (label optional)
/// DELETE /endpoints/{name}?label={label}     # Delete endpoint (label optional, * for all)
/// POST   /endpoints/{name}/execute?label={label}   # Execute endpoint (label optional)
/// ```
///
/// ### 3. Execution Tracking
/// Monitor and inspect workflow executions:
/// ```
/// GET /executions                    # List executions
/// GET /executions/{id}               # Get execution details
/// GET /executions/{id}/steps         # Get step-level results
/// ```
///
/// ### 4. Debug Mode
/// Step-by-step workflow debugging (layered on top of executions):
/// ```
/// POST   /execute                                     # Create execution (with debug=true)
/// POST   /executions/{id}/debug/step                  # Execute specific steps
/// POST   /executions/{id}/debug/continue              # Continue to completion
/// GET    /executions/{id}/debug/runnable              # Get runnable steps
/// GET    /executions/{id}/debug/state                 # Get debug state
/// ```
///
/// ## Key Concepts
///
/// ### Endpoints with Optional Labels
/// - **Endpoints**: Named workflow references with optional version labels
/// - **Labels**: Optional version identifiers (e.g., "v1.0", "stable", "beta")
/// - **Default Version**: Access without label parameter uses the default version
/// - **Unified Model**: Single concept instead of separate endpoints and labels
///
/// ### Content-based Deduplication
/// - Workflows are stored by SHA-256 hash for deduplication
/// - Multiple endpoints/labels can reference the same workflow
/// - Efficient storage and caching
///
/// ### Debug Mode
/// - Debug functionality layered on top of standard executions
/// - Step-by-step execution control via execution ID
/// - State inspection and manipulation
/// - Unified with execution tracking
///
/// ## Example Usage
///
/// ### Create and Execute Endpoint
/// ```bash
/// # Create default endpoint version
/// curl -X PUT http://localhost:7837/api/v1/endpoints/my-workflow \
///   -H "Content-Type: application/json" \
///   -d '{"workflow": {...}}'
///
/// # Create labeled version
/// curl -X PUT "http://localhost:7837/api/v1/endpoints/my-workflow?label=v1.0" \
///   -H "Content-Type: application/json" \
///   -d '{"workflow": {...}}'
///
/// # Execute default version
/// curl -X POST http://localhost:7837/api/v1/endpoints/my-workflow/execute \
///   -H "Content-Type: application/json" \
///   -d '{"input": {"key": "value"}}'
///
/// # Execute specific version
/// curl -X POST "http://localhost:7837/api/v1/endpoints/my-workflow/execute?label=v1.0" \
///   -H "Content-Type: application/json" \
///   -d '{"input": {"key": "value"}}'
/// ```
///
/// ### Debug Execution
/// ```bash
/// # Create debug execution
/// curl -X POST http://localhost:7837/api/v1/execute \
///   -H "Content-Type: application/json" \
///   -d '{"workflow": {...}, "input": {"key": "value"}, "debug": true}'
///
/// # List runnable steps
/// curl http://localhost:7837/api/v1/executions/{execution_id}/debug/runnable
///
/// # Execute specific steps
/// curl -X POST http://localhost:7837/api/v1/executions/{execution_id}/debug/step \
///   -H "Content-Type: application/json" \
///   -d '{"step_ids": ["step1", "step2"]}'
///
/// # Continue to completion
/// curl -X POST http://localhost:7837/api/v1/executions/{execution_id}/debug/continue
/// ```
///
/// ## Additional Features
///
/// - **Health Check**: `GET /health`
/// - **OpenAPI Spec**: `GET /openapi.json`
/// - **Comprehensive Error Handling**: Structured error responses
/// - **State Store Integration**: Persistent execution history
/// - **Plugin Architecture**: Extensible component system
///
/// ## Production Considerations
///
/// - **State Storage**: Configure SQLite or other backends for persistence
/// - **Security**: Add authentication/authorization as needed
/// - **Monitoring**: Built-in health checks and structured logging
/// - **Scaling**: Stateless design supports horizontal scaling
///
pub async fn serve(executor: Arc<StepFlowExecutor>, port: u16) -> Result<()> {
    // Use the experimental utoipa version for testing
    stepflow_server::start_server(port, executor)
        .await
        .map_err(|_e| crate::MainError::Configuration.into())
}
