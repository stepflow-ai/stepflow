use crate::{Result, error::MainError};
use std::sync::Arc;
use stepflow_core::FlowResult;
use stepflow_core::workflow::{Flow, ValueRef};
use stepflow_server::{CreateRunRequest, CreateRunResponse, StoreFlowRequest, StoreFlowResponse};
use url::Url;

/// Submit a workflow to a StepFlow service for execution
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
    }

    let store_result: StoreFlowResponse = store_response.json().await.map_err(|e| {
        tracing::error!("Failed to parse store response: {}", e);
        MainError::Configuration
    })?;

    // Step 2: Execute the workflow by hash
    let execute_request = CreateRunRequest {
        flow_hash: store_result.flow_hash,
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
