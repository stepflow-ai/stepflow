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

//! Type inference for workflows.
//!
//! This module provides the `stepflow infer` command which performs static
//! type checking on workflows and can output annotated flows with inferred schemas.

use crate::args::{ConfigArgs, WorkflowLoader, load};
use crate::schema_provider::PluginSchemaProvider;
use crate::{MainError, Result, validation_display::display_diagnostics};
use error_stack::ResultExt as _;
use serde_yaml_ng as serde_yaml;
use std::path::Path;
use stepflow_analysis::type_check_to_diagnostics;
use stepflow_core::workflow::Flow;
use stepflow_typecheck::{
    ComponentSchemaProvider as _, TypeCheckConfig, TypeEnvironment, build_input_schema,
    collect_input_constraints, find_constraint_conflicts, synthesize_type, type_check_flow,
};

/// Infer types for a workflow and output diagnostics.
///
/// Returns the number of type checking errors.
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub async fn infer(
    flow_path: &Path,
    config_path: Option<&Path>,
    output_path: Option<&Path>,
    strict: bool,
    show_step_inputs: bool,
) -> Result<u32> {
    // Load workflow
    let mut flow: Flow = load(flow_path)?;
    let flow_dir = flow_path.parent();

    // Load config and create executor to get plugin router
    let config = ConfigArgs {
        config_path: config_path.map(|p| p.to_path_buf()),
    }
    .load_config(flow_dir)?;

    let executor = WorkflowLoader::create_executor_from_config(config).await?;
    let plugin_router = executor.plugin_router();

    // Build schema provider from plugins
    let provider = PluginSchemaProvider::from_router(plugin_router).await;

    // Run type checking
    let config = TypeCheckConfig { strict };
    let result = type_check_flow(&flow, &provider, config);

    // Convert to diagnostics
    let diagnostics = type_check_to_diagnostics(&result);

    // Display diagnostics
    let failure_count = display_diagnostics(&diagnostics);

    // Show step input types if requested
    // This synthesizes the actual input expressions to show what types are being passed,
    // including any secret annotations propagated from variables.
    if show_step_inputs {
        eprintln!("\n📥 Synthesized step input types:");
        let env = TypeEnvironment::from_flow(&flow);
        for step in flow.steps() {
            let path = format!("steps.{}.input", step.id);
            let synthesis_result = synthesize_type(&step.input, &env, &path);
            eprintln!("  {}: {}", step.id, synthesis_result.ty);
        }
        eprintln!("\n📋 Component expected input types:");
        for step in flow.steps() {
            let input_type = provider
                .get_input_schema(&step.component)
                .map(|t| t.to_string())
                .unwrap_or_else(|| "any".to_string());
            eprintln!("  {}: {}", step.id, input_type);
        }
    }

    // Annotate the flow with inferred schemas
    // Step output schemas go into flow.schemas.steps
    let step_types = result.environment.step_types();

    // Collect step IDs that need output schemas
    let steps_to_update: Vec<_> = flow
        .steps()
        .iter()
        .filter_map(|step| {
            if flow.step_output_schema(&step.id).is_none() {
                step_types
                    .get(&step.id)
                    .and_then(|ty| ty.clone().into_schema())
                    .map(|schema| (step.id.clone(), schema))
            } else {
                None
            }
        })
        .collect();

    // Add step output schemas to types.steps
    for (step_id, schema) in steps_to_update {
        flow.set_step_output_schema(step_id, schema);
    }

    // Set flow output schema if not already set
    if flow.output_schema().is_none()
        && let Some(output_schema) = result.output_type.into_schema()
    {
        flow.set_output_schema(Some(output_schema));
    }

    // Infer input schema if not already set
    if flow.input_schema().is_none() {
        let constraints = collect_input_constraints(&flow, &provider);

        // Check for conflicting constraints
        let conflicts = find_constraint_conflicts(&constraints);
        if !conflicts.is_empty() {
            eprintln!("\n⚠️  Input constraint conflicts:");
            for conflict in &conflicts {
                eprintln!("  {}: {}", conflict.path, conflict.description);
            }
        }

        if let Some(input_schema) = build_input_schema(&constraints) {
            flow.set_input_schema(Some(input_schema));
        }
    }

    // Output the annotated flow
    if let Some(output_path) = output_path {
        let yaml = serde_yaml::to_string(&flow)
            .change_context(MainError::internal("Failed to serialize flow"))?;

        std::fs::write(output_path, yaml)
            .change_context(MainError::internal("Failed to write output file"))?;

        eprintln!("✅ Annotated flow written to: {}", output_path.display());
    } else {
        // Output to stdout
        let yaml = serde_yaml::to_string(&flow)
            .change_context(MainError::internal("Failed to serialize flow"))?;
        println!("{}", yaml);
    }

    // Report inferred step output types
    if !step_types.is_empty() {
        eprintln!("\n📊 Inferred step output types:");
        for (step_id, ty) in step_types {
            eprintln!("  {}: {}", step_id, ty);
        }
    }

    Ok(failure_count)
}
