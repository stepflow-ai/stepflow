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

use std::sync::Arc;

use crate::{MainError, Result};
use error_stack::ResultExt as _;
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
/// ## Currently Implemented Endpoints
///
/// ### 1. Health Check
/// ```
/// GET /health                        # Check service health and status
/// ```
///
/// ### 2. Component Management
/// ```
/// GET /components                    # List all available components
///     ?includeSchemas=true           # Include input/output schemas (default: true)
/// ```
///
/// ### 3. Flow Management
/// Store and retrieve workflow definitions by content-based hash:
/// ```
/// POST /flows                        # Store workflow definition, get hash
/// GET  /flows/{flow_hash}            # Retrieve workflow by hash
/// DELETE /flows/{flow_hash}          # Delete workflow (placeholder)
/// ```
///
/// ### 4. Execution Management
/// Create and manage workflow executions:
/// ```
/// POST /runs                         # Create and execute workflow by hash
/// GET  /runs                         # List all executions
/// GET  /runs/{run_id}                # Get execution details
/// GET  /runs/{run_id}/flow           # Get workflow definition for execution
/// GET  /runs/{run_id}/steps          # Get step-level execution details
/// POST /runs/{run_id}/cancel         # Cancel running execution (placeholder)
/// DELETE /runs/{run_id}              # Delete completed execution (placeholder)
/// ```
///
/// ### 5. Debug Mode
/// Step-by-step execution control for debugging:
/// ```
/// POST /runs/{run_id}/debug/step     # Execute specific steps in debug mode
/// POST /runs/{run_id}/debug/continue # Continue debug execution to completion
/// GET  /runs/{run_id}/debug/runnable # Get list of runnable steps
/// ```
///
/// ## Key Concepts
///
/// ### Content-based Flow Storage
/// - Workflows are stored by SHA-256 hash of their definition
/// - Same workflow definition always produces the same hash
/// - Enables efficient deduplication and caching
/// - Flow hash is returned when storing via `POST /flows`
///
/// ### Two-step Execution Pattern
/// 1. **Store workflow**: `POST /flows` with workflow definition → get `flowHash`
/// 2. **Execute workflow**: `POST /runs` with `flowHash` and input data → get execution results
///
/// ### Debug Mode
/// - Create execution with `debug: true` in `POST /runs`
/// - Execution pauses, allowing step-by-step control
/// - Use debug endpoints to execute specific steps or continue
/// - Full state inspection and manipulation capabilities
///
/// ## Example Usage
///
/// ### Basic Workflow Execution
/// ```bash
/// # 1. Store workflow definition
/// curl -X POST http://localhost:7837/api/v1/flows \
///   -H "Content-Type: application/json" \
///   -d '{"flow": {"steps": [...], "output": {...}}}'
/// # Response: {"flowHash": "abc123...", "analysis": {...}}
///
/// # 2. Execute workflow by hash
/// curl -X POST http://localhost:7837/api/v1/runs \
///   -H "Content-Type: application/json" \
///   -d '{"flowHash": "abc123...", "input": {"key": "value"}}'
/// # Response: {"runId": "uuid", "result": {...}, "status": "completed"}
/// ```
///
/// ### Debug Execution
/// ```bash
/// # Create debug execution (paused)
/// curl -X POST http://localhost:7837/api/v1/runs \
///   -H "Content-Type: application/json" \
///   -d '{"flowHash": "abc123...", "input": {"key": "value"}, "debug": true}'
///
/// # Get runnable steps
/// curl http://localhost:7837/api/v1/runs/{run_id}/debug/runnable
///
/// # Execute specific steps
/// curl -X POST http://localhost:7837/api/v1/runs/{run_id}/debug/step \
///   -H "Content-Type: application/json" \
///   -d '{"stepIds": ["step1", "step2"]}'
///
/// # Continue to completion
/// curl -X POST http://localhost:7837/api/v1/runs/{run_id}/debug/continue
/// ```
///
/// ### Monitoring and Inspection
/// ```bash
/// # List all executions
/// curl http://localhost:7837/api/v1/runs
///
/// # Get execution details
/// curl http://localhost:7837/api/v1/runs/{run_id}
///
/// # Get step-level results
/// curl http://localhost:7837/api/v1/runs/{run_id}/steps
///
/// # Get workflow definition used
/// curl http://localhost:7837/api/v1/runs/{run_id}/flow
/// ```
///
/// ## Additional Features
///
/// - **OpenAPI Specification**: `GET /api/v1/openapi.json`
/// - **Swagger UI**: Available at `/swagger-ui` (when enabled)
/// - **CORS Support**: Cross-origin requests enabled by default
/// - **Structured Error Handling**: Consistent error response format
/// - **State Store Integration**: Configurable persistence (in-memory, SQLite)
/// - **Plugin Architecture**: Extensible component system with routing
///
/// ## Response Formats
///
/// All endpoints return structured JSON responses with appropriate HTTP status codes:
/// - **2xx**: Success responses with requested data
/// - **4xx**: Client errors (invalid input, not found, etc.)
/// - **5xx**: Server errors with error details
///
/// Error responses follow the format:
/// ```json
/// {"code": 404, "message": "Execution 'uuid' not found"}
/// ```
///
/// ## Production Considerations
///
/// - **State Storage**: Configure SQLite or other backends for persistence
/// - **Security**: Add authentication/authorization as needed
/// - **Monitoring**: Built-in health checks and structured logging
/// - **Scaling**: Stateless design supports horizontal scaling
/// - **Performance**: Content-based caching and efficient execution engine
///
pub async fn serve(executor: Arc<StepFlowExecutor>, port: u16) -> Result<()> {
    // Use the experimental utoipa version for testing
    stepflow_server::start_server(port, executor)
        .await
        .map_err(Arc::<dyn std::error::Error + Send + Sync>::from)
        .change_context(MainError::ServerError)
}
