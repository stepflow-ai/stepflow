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

use crate::args::{ConfigArgs, WorkflowLoader, load};
use crate::schema_provider::PluginSchemaProvider;
use crate::{MainError, Result, validation_display::display_diagnostics};
use error_stack::ResultExt as _;
use std::path::Path;
use stepflow_analysis::{type_check_to_diagnostics, validate_with_config};
use stepflow_typecheck::{TypeCheckConfig, type_check_flow};

/// Validate workflow files and configuration
///
/// Returns the number of validation failures (errors + fatal diagnostics).
pub async fn validate(
    flow_path: &Path,
    config_path: Option<&Path>,
    type_check: bool,
    strict: bool,
) -> Result<u32> {
    // Load workflow and configuration
    let flow = load(flow_path)?;
    let flow_dir = flow_path.parent();

    let config = if let Some(config_path) = config_path {
        // Load specific config file
        ConfigArgs {
            config_path: Some(config_path.to_path_buf()),
        }
        .load_config(flow_dir)?
    } else {
        // Auto-discover config
        ConfigArgs { config_path: None }.load_config(flow_dir)?
    };

    // Use unified validation
    let mut diagnostics = validate_with_config(&flow, &config.plugins, &config.routing)
        .change_context(MainError::ValidationError("Validation failed".to_string()))?;

    // Optionally run type checking
    if type_check {
        // Create executor to get plugin router for schema lookups
        let executor = WorkflowLoader::create_executor_from_config(config).await?;
        let plugin_router = executor.plugin_router();
        let provider = PluginSchemaProvider::from_router(plugin_router).await;

        // Run type checking
        let type_config = TypeCheckConfig { strict };
        let result = type_check_flow(&flow, &provider, type_config);

        // Convert type errors to diagnostics and combine
        let type_diagnostics = type_check_to_diagnostics(&result);
        diagnostics.extend(type_diagnostics);
    }

    // Display results using the same function as other commands
    let failure_count = display_diagnostics(&diagnostics);

    Ok(failure_count)
}
