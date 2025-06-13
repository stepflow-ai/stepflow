use crate::{Result, error::MainError};
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_server::executions::{CreateExecutionRequest, CreateExecutionResponse};
use url::Url;

/// Submit a workflow to a StepFlow service for execution
pub async fn submit(service_url: Url, flow: Flow, input: ValueRef) -> Result<FlowResult> {
    // Create the request payload using server struct
    let request = CreateExecutionRequest {
        workflow: Arc::new(flow),
        input,
        debug: false, // TODO: Add debug option to CLI
    };

    // Build the execute endpoint URL (correct path)
    let execute_url = service_url
        .join("/api/v1/executions")
        .map_err(|_| MainError::Configuration)?;

    // Create HTTP client
    let client = reqwest::Client::new();

    // Send the request
    let response = client
        .post(execute_url)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Failed to send request: {}", e);
            MainError::Configuration
        })?;

    // Check if the request was successful
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        tracing::error!("Server returned error {}: {}", status, error_text);
        return Err(MainError::Configuration.into());
    }

    // Parse the response using server struct
    let execute_response: CreateExecutionResponse = response.json().await.map_err(|e| {
        tracing::error!("Failed to parse response: {}", e);
        MainError::Configuration
    })?;

    // Return the result if available
    match execute_response.result {
        Some(result) => Ok(result),
        None => {
            tracing::error!("No result in response");
            Err(MainError::Configuration.into())
        }
    }
}
