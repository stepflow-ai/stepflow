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

//! Basic example demonstrating logging with automatic trace context injection

use stepflow_observability::{
    BinaryObservabilityConfig, LogDestination, LogFormat, ObservabilityConfig,
    fastrace::prelude::*, init_observability,
};

fn main() {
    let config = ObservabilityConfig {
        log_level: log::LevelFilter::Debug,
        log_format: LogFormat::Json,
        log_destination: LogDestination::Stdout,
        trace_enabled: true,
        binary_config: BinaryObservabilityConfig::default(), // No run diagnostic
        otlp_endpoint: None,
        service_name: "example".to_string(),
    };

    let _guard = init_observability(config).unwrap();

    // Test basic logging
    log::info!("Starting example - no trace context");

    // Create a root span using LocalSpan which automatically sets itself as parent
    {
        let root = Span::root("worker-loop", SpanContext::random());
        let _guard = root.set_local_parent();

        // Check if span context is available
        if let Some(ctx) = fastrace::collector::SpanContext::current_local_parent() {
            eprintln!(
                "DEBUG: Root span context found - trace_id={:032x} span_id={:016x}",
                ctx.trace_id.0, ctx.span_id.0
            );
        } else {
            eprintln!("DEBUG: No root span context found!");
        }

        log::info!("Inside root span");

        // Nested span
        {
            let _span = LocalSpan::enter_with_local_parent("example_operation");
            log::info!("Inside nested span");
            log::debug!("Debug info inside nested span");

            // Double nested span
            {
                let _span2 = LocalSpan::enter_with_local_parent("nested_operation");
                log::info!("Inside double nested span");
                log::warn!("Warning in double nested span");
            }
        }

        log::info!("Back at root level");
    }

    log::info!("After root span closed");

    // Note: The guard will flush traces when dropped
}
