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

// Allow println for CLI output
#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use crate::{
    Result, error::MainError, submit::display_server_error, validation_display::display_diagnostics,
};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::io::{BufRead as _, BufReader};
use std::path::Path;
use std::time::Instant;
use stepflow_core::{
    BlobId,
    workflow::{Flow, ValueRef},
};
use stepflow_server::{StoreFlowRequest, StoreFlowResponse};
use stepflow_state::BatchDetails;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBatchRequest {
    flow_id: BlobId,
    inputs: Vec<ValueRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_concurrency: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBatchResponse {
    batch_id: Uuid,
    total_inputs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBatchResponse {
    #[serde(flatten)]
    details: BatchDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchOutputInfo {
    batch_input_index: usize,
    status: stepflow_core::status::ExecutionStatus,
    result: Option<stepflow_core::FlowResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListBatchOutputsResponse {
    outputs: Vec<BatchOutputInfo>,
}

/// Submit a batch workflow to a Stepflow service for execution
pub async fn submit_batch(
    service_url: Url,
    flow: std::sync::Arc<Flow>,
    inputs_path: &Path,
    max_concurrent: Option<usize>,
    output_path: Option<&Path>,
) -> Result<()> {
    let client = reqwest::Client::new();

    println!("============================================================");
    println!("Batch Submission");
    println!("============================================================");
    println!("Flow: {}", flow.name().unwrap_or("(unnamed)"));
    println!("Inputs file: {}", inputs_path.display());
    println!("Server: {}", service_url);

    // Read and parse JSONL inputs
    let inputs = read_jsonl_inputs(inputs_path)?;
    let total_inputs = inputs.len();
    println!("Total inputs: {}", total_inputs);

    if total_inputs == 0 {
        println!("\n⚠️  No inputs found in file");
        return Ok(());
    }

    if let Some(max) = max_concurrent {
        println!("Max concurrency: {}", max);
    }
    println!();

    // Step 1: Store the flow to get its hash
    println!("Storing workflow...");
    let store_request = StoreFlowRequest { flow };

    let store_url = service_url
        .join("/api/v1/flows")
        .map_err(|_| MainError::Configuration)?;

    let store_response = client
        .post(store_url)
        .json(&store_request)
        .send()
        .await
        .change_context(MainError::Configuration)?;

    if !store_response.status().is_success() {
        let status = store_response.status();
        let error_text = store_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        display_server_error(status, &error_text, "storing workflow");
        return Err(MainError::Configuration.into());
    }

    let store_result: StoreFlowResponse = store_response
        .json()
        .await
        .change_context(MainError::ServerError)?;

    // Display validation results
    let failure_count = display_diagnostics(&store_result.diagnostics);

    // Check if the workflow was stored successfully
    let flow_id = store_result.flow_id.ok_or_else(|| {
        if failure_count > 0 {
            log::error!("Workflow validation failed - see diagnostics above");
        } else {
            log::error!("Workflow was not stored for unknown reasons");
        }
        MainError::ValidationError("Workflow validation failed".to_string())
    })?;

    println!("✅ Workflow stored: {}", flow_id);
    println!();

    // Step 2: Create batch execution
    println!("Creating batch...");
    let create_batch_request = CreateBatchRequest {
        flow_id: flow_id.clone(),
        inputs,
        max_concurrency: max_concurrent,
    };

    let create_batch_url = service_url
        .join("/api/v1/batches")
        .map_err(|_| MainError::Configuration)?;

    let create_batch_response = client
        .post(create_batch_url)
        .json(&create_batch_request)
        .send()
        .await
        .change_context(MainError::Configuration)?;

    if !create_batch_response.status().is_success() {
        let status = create_batch_response.status();
        let error_text = create_batch_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        display_server_error(status, &error_text, "creating batch");
        return Err(MainError::Configuration.into());
    }

    let create_batch_result: CreateBatchResponse = create_batch_response
        .json()
        .await
        .change_context(MainError::ServerError)?;

    let batch_id = create_batch_result.batch_id;
    println!("Batch ID: {}", batch_id);
    println!();

    // Step 3: Poll for progress
    println!("Executing...");
    let start_time = Instant::now();
    let get_batch_url = service_url
        .join(&format!("/api/v1/batches/{}", batch_id))
        .map_err(|_| MainError::Configuration)?;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let response = client
            .get(get_batch_url.clone())
            .send()
            .await
            .change_context(MainError::Configuration)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            display_server_error(status, &error_text, "getting batch status");
            return Err(MainError::Configuration.into());
        }

        let batch_response: GetBatchResponse = response
            .json()
            .await
            .change_context(MainError::ServerError)?;

        let stats = &batch_response.details.statistics;

        // Simple progress display
        let completed = stats.completed_runs;
        let running = stats.running_runs;
        let failed = stats.failed_runs;
        let bar_width = 40;
        let filled = (completed as f64 / total_inputs as f64 * bar_width as f64) as usize;
        let bar: String =
            "=".repeat(filled) + ">" + &" ".repeat(bar_width - filled.saturating_sub(1));

        print!(
            "\r[{}] {}/{} completed, {} running, {} failed",
            bar, completed, total_inputs, running, failed
        );
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        // Check if all runs are complete
        if completed + failed >= total_inputs {
            break;
        }
    }

    println!(); // New line after progress bar
    let duration = start_time.elapsed();

    // Get final statistics
    let final_response = client
        .get(get_batch_url)
        .send()
        .await
        .change_context(MainError::Configuration)?;

    if !final_response.status().is_success() {
        let status = final_response.status();
        let error_text = final_response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        display_server_error(status, &error_text, "getting final batch status");
        return Err(MainError::Configuration.into());
    }

    let final_batch: GetBatchResponse = final_response
        .json()
        .await
        .change_context(MainError::ServerError)?;

    let final_stats = &final_batch.details.statistics;

    // Display final statistics
    println!();
    println!("Final Statistics:");
    println!("  Total: {}", total_inputs);
    println!("  ✅ Completed: {}", final_stats.completed_runs);
    println!("  ❌ Failed: {}", final_stats.failed_runs);
    println!("  ⏱️  Duration: {:.1}s", duration.as_secs_f64());
    println!("============================================================");

    // Write output to file if requested
    if let Some(output_path) = output_path {
        println!("Fetching results...");

        // Get all batch outputs in a single API call
        let outputs_url = service_url
            .join(&format!("/api/v1/batches/{}/outputs", batch_id))
            .map_err(|_| MainError::Configuration)?;

        let outputs_response = client
            .get(outputs_url)
            .send()
            .await
            .change_context(MainError::Configuration)?;

        if !outputs_response.status().is_success() {
            let status = outputs_response.status();
            let error_text = outputs_response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            display_server_error(status, &error_text, "getting batch outputs");
            return Err(MainError::Configuration.into());
        }

        let outputs_result: ListBatchOutputsResponse = outputs_response
            .json()
            .await
            .change_context(MainError::ServerError)?;

        // Write JSONL output (one JSON object per line)
        let mut output_lines = Vec::new();
        for output_info in outputs_result.outputs {
            let output_entry = serde_json::json!({
                "index": output_info.batch_input_index,
                "status": output_info.status,
                "result": output_info.result,
            });
            output_lines.push(
                serde_json::to_string(&output_entry).change_context(MainError::Configuration)?,
            );
        }

        std::fs::write(output_path, output_lines.join("\n") + "\n")
            .change_context(MainError::Configuration)
            .attach_printable(format!(
                "Failed to write output file: {}",
                output_path.display()
            ))?;

        println!(
            "Output written to: {} ({} results)",
            output_path.display(),
            output_lines.len()
        );
    }

    // Exit with error code if any failures
    if final_stats.failed_runs > 0 {
        return Err(error_stack::report!(MainError::ServerError)
            .attach_printable(format!("{} runs failed", final_stats.failed_runs)));
    }

    Ok(())
}

/// Read and parse JSONL file (one JSON object per line)
fn read_jsonl_inputs(path: &Path) -> Result<Vec<ValueRef>> {
    let file = std::fs::File::open(path)
        .change_context(MainError::Configuration)
        .attach_printable(format!("Failed to open input file: {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut inputs = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.change_context(MainError::Configuration)?;
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let value: serde_json::Value = serde_json::from_str(line)
            .change_context(MainError::Configuration)
            .attach_printable(format!("Failed to parse JSON on line {}", line_num + 1))?;

        inputs.push(ValueRef::new(value));
    }

    Ok(inputs)
}
