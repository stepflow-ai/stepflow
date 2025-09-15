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

use crate::{Result, error::MainError, validation_display::display_diagnostics};
use error_stack::ResultExt as _;
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_server::{CreateRunRequest, CreateRunResponse, StoreFlowRequest, StoreFlowResponse};
use url::Url;

/// Submit a workflow to a Stepflow service for execution
pub async fn submit(service_url: Url, flow: Flow, input: ValueRef) -> Result<FlowResult> {
    let client = reqwest::Client::new();

    // Step 1: Store the flow to get its hash
    let store_request = StoreFlowRequest {
        flow: Arc::new(flow),
    };

    let store_url = service_url
        .join("/api/v1/flows")
        .map_err(|_| MainError::Configuration)?;

    let store_response = client
        .post(store_url)
        .json(&store_request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to store workflow: {}", e);
            MainError::Configuration
        })?;

    if !store_response.status().is_success() {
        let status = store_response.status();
        let error_text = store_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        tracing::error!(
            "Server returned error storing workflow {}: {}",
            status,
            error_text
        );
        return Err(MainError::Configuration.into());
    };

    let store_result: StoreFlowResponse = store_response
        .json()
        .await
        .change_context(MainError::ServerError)?;

    // Display validation results
    let failure_count = display_diagnostics(&store_result.analysis_result.diagnostics);

    // Check if the workflow was stored successfully
    let flow_id = store_result.flow_id.ok_or_else(|| {
        // If validation failed, the error details were already shown by display_diagnostics
        if failure_count > 0 {
            tracing::error!("Workflow validation failed - see diagnostics above");
        } else {
            tracing::error!("Workflow was not stored for unknown reasons");
        }
        MainError::ValidationError("Workflow validation failed".to_string())
    })?;

    // Step 2: Execute the workflow by hash
    let execute_request = CreateRunRequest {
        flow_id,
        input,
        debug: false, // TODO: Add debug option to CLI
    };

    let execute_url = service_url
        .join("/api/v1/runs")
        .map_err(|_| MainError::Configuration)?;

    let execute_response = client
        .post(execute_url)
        .json(&execute_request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute workflow: {}", e);
            MainError::Configuration
        })?;

    if !execute_response.status().is_success() {
        let status = execute_response.status();
        let error_text = execute_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        tracing::error!(
            "Server returned error executing workflow {}: {}",
            status,
            error_text
        );
        return Err(MainError::Configuration.into());
    }

    let execute_result: CreateRunResponse = execute_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse execute response: {}", e);
        MainError::Configuration
    })?;

    // Return the result if available
    match execute_result.result {
        Some(result) => Ok(result),
        None => {
            tracing::error!("No result in response");
            Err(MainError::Configuration.into())
        }
    }
}
