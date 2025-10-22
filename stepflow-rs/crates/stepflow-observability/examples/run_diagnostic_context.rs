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

//! Example demonstrating automatic injection of run_id and step_id
//! along with trace context into all log records

use stepflow_observability::{
    BinaryObservabilityConfig, LogDestinationType, LogFormat, ObservabilityConfig, RunInfoGuard,
    StepIdGuard, fastrace::prelude::*, init_observability,
};

#[tokio::main]
async fn main() {
    let config = ObservabilityConfig {
        log_level: log::LevelFilter::Debug,
        other_log_level: None,
        log_destination: LogDestinationType::Stdout,
        log_format: LogFormat::Json,
        log_file: None,
        trace_enabled: false,
        otlp_endpoint: None,
    };

    let binary_config = BinaryObservabilityConfig {
        service_name: "example",
        include_run_diagnostic: true,
    };
    let guard = init_observability(&config, binary_config).unwrap();

    // Log without any context
    log::info!("Starting workflow execution - no context yet");

    // Simulate workflow execution with run_id
    {
        // Set run_id for this workflow run
        let _run_guard = RunInfoGuard::new("flow-1", "run-12345");

        log::info!("Workflow started");

        // Create a root trace span for the workflow execution
        {
            let root = Span::root("workflow-execution", SpanContext::random());
            let _span_guard = root.set_local_parent();

            log::info!("Inside trace span - should have both trace_id and run_id");

            // Simulate step execution
            execute_step("step1");
            execute_step("step2");

            log::info!("Workflow completed");
        }

        log::info!("After trace span closed - still has run_id");
    }

    log::info!("After run_id cleared - no context");

    // Explicitly close the guard to flush telemetry
    guard
        .close()
        .await
        .expect("Failed to flush observability data");
}

fn execute_step(step_name: &'static str) {
    // Set step_id for this step execution
    let _step_guard = StepIdGuard::new(step_name);

    // Create a child span for this step
    let _span = LocalSpan::enter_with_local_parent(step_name);

    log::info!("Step started");
    log::debug!("Processing step logic");
    log::info!("Step completed");
}
