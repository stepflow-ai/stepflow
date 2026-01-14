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

#![allow(clippy::print_stdout)]

use crate::args::{ConfigArgs, WorkflowLoader};
use crate::{MainError, Result};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use stepflow_core::component::ComponentInfo;
use stepflow_plugin::routing::RouteMatch;
use stepflow_plugin::{Plugin as _, PluginRouterExt as _};

/// Output format for command line display
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Pretty,
    Json,
    Yaml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComponentWithRoutes {
    #[serde(flatten)]
    pub component_info: ComponentInfo,
    /// The plugin that provides this component
    pub plugin: String,
    /// Available routes for accessing this component
    pub routes: Vec<RouteMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComponentList {
    pub components: Vec<ComponentWithRoutes>,
}

/// List all available components from a stepflow config.
pub async fn list_components(
    config_path: Option<PathBuf>,
    format: OutputFormat,
    schemas: Option<bool>,
    hide_unreachable: bool,
) -> Result<()> {
    // Determine whether to include schemas based on format and explicit flag
    let include_schemas = schemas.unwrap_or(match format {
        OutputFormat::Pretty => false,
        OutputFormat::Json | OutputFormat::Yaml => true,
    });
    // Load config using the standard resolution logic
    let config_args = ConfigArgs::with_path(config_path);
    let config = config_args.load_config(None)?;

    // Create executor to instantiate plugins
    let executor = WorkflowLoader::create_executor_from_config(config).await?;

    // Get all registered plugins and query their components with routing information
    let mut all_components = Vec::new();

    // Get the plugin router to access reverse routing functionality
    let plugin_router = executor.plugin_router();

    // Iterate over plugins with their names
    for (plugin_name, plugin) in plugin_router.plugins_with_names() {
        // List components available from this plugin
        let components = plugin
            .list_components()
            .await
            .change_context(MainError::PluginCommunication)?;

        // For each component, find its routes using reverse routing
        for mut info in components {
            if !include_schemas {
                info.input_schema = None;
                info.output_schema = None;
            }

            let component_name = info.component.path();

            // Find routes for this plugin/component combination
            let routes = plugin_router.find_routes_for(plugin_name, component_name);

            all_components.push(ComponentWithRoutes {
                component_info: info,
                plugin: plugin_name.to_string(),
                routes,
            });
        }
    }

    // Filter out unreachable components if requested
    if hide_unreachable {
        all_components.retain(|component| !component.routes.is_empty());
    }

    // Sort components by their URL for consistent output
    all_components.sort_by(|a, b| {
        a.component_info
            .component
            .to_string()
            .cmp(&b.component_info.component.to_string())
    });

    let component_list = ComponentList {
        components: all_components,
    };

    // Output in the requested format
    match format {
        OutputFormat::Pretty => print_pretty(&component_list.components),
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&component_list)
                .change_context(MainError::SerializationError)?;
            println!("{json}");
        }
        OutputFormat::Yaml => {
            let yaml = serde_yaml_ng::to_string(&component_list)
                .change_context(MainError::SerializationError)?;
            println!("{yaml}");
        }
    }

    Ok(())
}

fn print_pretty(components: &[ComponentWithRoutes]) {
    if components.is_empty() {
        println!("No components found.");
        return;
    }

    println!("Available Components:");
    println!("====================");

    for component in components {
        println!();
        println!(
            "Component: {} (plugin: {})",
            component.component_info.component, component.plugin
        );

        // Print description if present
        if let Some(ref description) = component.component_info.description {
            println!("  Description: {description}");
        }

        // Print available routes
        if !component.routes.is_empty() {
            println!("  Available Routes:");
            for route in &component.routes {
                println!("    {}", route.resolved_path);
                if !route.conditions.is_empty() {
                    println!("      Conditions:");
                    for condition in &route.conditions {
                        println!("        {}: {:?}", condition.path, condition.value);
                    }
                }
            }
        } else {
            println!("  Available Routes: None");
        }

        // Print input schema if present
        if let Some(ref input_schema) = component.component_info.input_schema {
            println!("  Input Schema:");
            let input_schema_str = serde_json::to_string_pretty(input_schema)
                .unwrap_or_else(|_| "Error serializing schema".to_string());
            for line in input_schema_str.lines() {
                println!("    {line}");
            }
        }

        // Print output schema if present
        if let Some(ref output_schema) = component.component_info.output_schema {
            println!("  Output Schema:");
            let output_schema_str = serde_json::to_string_pretty(output_schema)
                .unwrap_or_else(|_| "Error serializing schema".to_string());
            for line in output_schema_str.lines() {
                println!("    {line}");
            }
        }
    }

    println!();
    println!("Total components: {}", components.len());
}
