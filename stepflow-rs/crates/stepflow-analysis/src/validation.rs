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

use indexmap::IndexMap;
use stepflow_core::workflow::Flow;
use stepflow_plugin::routing::RoutingConfig;

use crate::Result;
use crate::diagnostics::Diagnostics;

mod components;
mod config;
mod flow_structure;
mod path;
mod reachable;
mod references;
mod subflows;
pub use path::Path;
pub(crate) use path::{PathPart, make_path};

#[cfg(test)]
mod tests;

/// Validates a workflow and collects all diagnostics
pub fn validate(flow: &Flow) -> Result<Diagnostics> {
    let mut diagnostics = Diagnostics::new();
    flow_structure::validate_flow_structure(flow, &mut diagnostics);
    references::validate_references(flow, &mut diagnostics);
    components::validate_components(flow, &mut diagnostics);
    reachable::validate_step_reachability(flow, &mut diagnostics)?;

    subflows::validate_literal_subflows(flow, &mut diagnostics)?;
    Ok(diagnostics)
}

/// Validate both workflow and configuration together
pub fn validate_with_config(
    flow: &Flow,
    plugins: &IndexMap<String, impl std::fmt::Debug>,
    routing: &RoutingConfig,
) -> Result<Diagnostics> {
    let mut diagnostics = validate(flow)?;
    config::validate_config(plugins, routing, &mut diagnostics);
    Ok(diagnostics)
}
