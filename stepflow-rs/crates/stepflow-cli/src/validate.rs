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

use crate::args::{ConfigArgs, load};
use crate::{MainError, Result, validation_display::display_diagnostics};
use error_stack::ResultExt as _;
use std::path::Path;
use stepflow_analysis::validate_with_config;

/// Validate workflow files and configuration
///
/// Returns the number of validation failures (errors + fatal diagnostics).
pub async fn validate(flow_path: &Path, config_path: Option<&Path>) -> Result<usize> {
    // Load workflow and configuration
    let flow = load(flow_path)?;

    let config = if let Some(config_path) = config_path {
        // Load specific config file
        ConfigArgs {
            config_path: Some(config_path.to_path_buf()),
        }
        .load_config(flow_path.parent())?
    } else {
        // Auto-discover config
        ConfigArgs { config_path: None }.load_config(flow_path.parent())?
    };

    // Use unified validation
    let diagnostics = validate_with_config(&flow, &config.plugins, &config.routing)
        .change_context(MainError::ValidationError("Validation failed".to_string()))?;

    // Display results using the same function as other commands
    let failure_count = display_diagnostics(&diagnostics);

    Ok(failure_count)
}
