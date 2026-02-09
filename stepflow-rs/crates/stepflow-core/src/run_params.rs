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

use crate::workflow::{ValueRef, WorkflowOverrides};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Optional parameters for submitting a run.
///
/// Pass to `submit_run()` along with the required flow, flow_id, and inputs.
/// Use `Default::default()` for simple submissions with no extra options.
///
/// # Example
/// ```ignore
/// // Simple submission with defaults
/// submit_run(&env, flow, flow_id, inputs, SubmitRunParams::default()).await?;
///
/// // With options
/// submit_run(&env, flow, flow_id, inputs, SubmitRunParams {
///     max_concurrency: Some(4),
///     overrides,
///     ..Default::default()
/// }).await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct SubmitRunParams {
    pub max_concurrency: Option<usize>,
    /// Step overrides to apply during execution. Empty means no overrides.
    pub overrides: WorkflowOverrides,
    pub variables: Option<HashMap<String, ValueRef>>,
}

/// Parameters for getting run status and results.
///
/// Use `wait_for_completion()` separately if you need to wait for a run to finish.
///
/// # Example
/// ```ignore
/// // Get status only
/// get_run(&env, run_id, GetRunParams::default()).await?;
///
/// // Get status with results
/// get_run(&env, run_id, GetRunParams { include_results: true, ..Default::default() }).await?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct GetRunParams {
    /// Include item results in the response.
    pub include_results: bool,
    /// Order of results (only relevant if include_results=true).
    pub result_order: ResultOrder,
}

/// Order in which to return results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultOrder {
    /// Results in input order (index 0, 1, 2, ...).
    #[default]
    ByIndex,
    /// Results in completion order (first completed first).
    ByCompletion,
}
