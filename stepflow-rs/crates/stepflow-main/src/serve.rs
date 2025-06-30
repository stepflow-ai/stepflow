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
/// Execute workflows directly by hash or definition:
/// ```
/// POST /executions                           # Execute workflow with definition
/// POST /workflows/{hash}/execute             # Execute workflow by hash
/// ```
///
/// ### 2. Named Workflow Management with Labels
/// Manage workflows by name with optional version labels:
/// ```
/// GET    /workflows/names                              # List all workflow names
/// GET    /workflows/by-name/{name}                     # List all versions of named workflow
/// GET    /workflows/by-name/{name}/latest              # Get latest version of named workflow
/// POST   /workflows/by-name/{name}/execute             # Execute latest version
/// GET    /workflows/by-name/{name}/labels              # List labels for workflow
/// PUT    /workflows/by-name/{name}/labels/{label}      # Create/update labeled version
/// GET    /workflows/by-name/{name}/labels/{label}      # Get labeled version
/// POST   /workflows/by-name/{name}/labels/{label}/execute  # Execute labeled version
/// DELETE /workflows/by-name/{name}/labels/{label}      # Delete labeled version
/// ```
///
/// ### 3. Execution Tracking
/// Monitor and inspect workflow executions:
/// ```
/// GET /executions                    # List executions
/// GET /executions/{id}               # Get execution details
/// GET /executions/{id}/steps         # Get step-level results
/// GET /executions/{id}/workflow      # Get execution workflow
/// ```
///
/// ### 4. Debug Mode
/// Step-by-step workflow debugging:
/// ```
/// POST   /executions                                  # Create execution (with debug=true)
/// POST   /executions/{id}/debug/step                  # Execute specific steps
/// POST   /executions/{id}/debug/continue              # Continue to completion
/// GET    /executions/{id}/debug/runnable              # Get runnable steps
/// ```
///
/// ## Key Concepts
///
/// ### Three Workflow Access Patterns
/// 1. **Ad-hoc workflows**: Execute by hash or definition directly
/// 2. **Named workflows**: Workflows with a `name` field, accessible by name
/// 3. **Labeled workflows**: Named workflows with version labels (e.g., "production", "staging", "v1.0")
///
/// ### Content-based Deduplication
/// - Workflows are stored by SHA-256 hash for deduplication
/// - Multiple names/labels can reference the same workflow
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
/// ### Create and Execute Named Workflow
/// ```bash
/// # Store a workflow (creates hash)
/// curl -X POST http://localhost:7837/api/v1/workflows \
///   -H "Content-Type: application/json" \
///   -d '{"workflow": {"name": "my-workflow", "steps": [...]}}'
///
/// # Create labeled version
/// curl -X PUT http://localhost:7837/api/v1/workflows/by-name/my-workflow/labels/v1.0 \
///   -H "Content-Type: application/json" \
///   -d '{"workflow_hash": "abc123..."}'
///
/// # Execute latest version
/// curl -X POST http://localhost:7837/api/v1/workflows/by-name/my-workflow/execute \
///   -H "Content-Type: application/json" \
///   -d '{"input": {"key": "value"}}'
///
/// # Execute specific version
/// curl -X POST http://localhost:7837/api/v1/workflows/by-name/my-workflow/labels/v1.0/execute \
///   -H "Content-Type: application/json" \
///   -d '{"input": {"key": "value"}}'
/// ```
///
/// ### Debug Execution
/// ```bash
/// # Create debug execution
/// curl -X POST http://localhost:7837/api/v1/executions \
///   -H "Content-Type: application/json" \
///   -d '{"workflow": {...}, "input": {"key": "value"}, "debug": true}'
///
/// # List runnable steps
/// curl http://localhost:7837/api/v1/runs/{run_id}/debug/runnable
///
/// # Execute specific steps
/// curl -X POST http://localhost:7837/api/v1/runs/{run_id}/debug/step \
///   -H "Content-Type: application/json" \
///   -d '{"step_ids": ["step1", "step2"]}'
///
/// # Continue to completion
/// curl -X POST http://localhost:7837/api/v1/runs/{run_id}/debug/continue
/// ```
///
/// ## Additional Features
///
/// - **Health Check**: `GET /health`
/// - **OpenAPI Spec**: `GET /openapi.json`
/// - **Components**: `GET /components` for available plugins
/// - **Workflow Dependencies**: `GET /workflows/{hash}/dependencies`
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
