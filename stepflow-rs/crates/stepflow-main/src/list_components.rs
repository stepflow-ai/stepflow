#![allow(clippy::print_stdout)]
// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use crate::args::{ConfigArgs, WorkflowLoader};
use crate::{MainError, Result};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use stepflow_core::schema::SchemaRef;
use stepflow_core::workflow::Component;
use stepflow_plugin::Plugin as _;

/// Output format for command line display
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Pretty,
    Json,
    Yaml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComponentDetails {
    /// The component identifier (URL)
    pub component: Component,
    /// Input schema (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<SchemaRef>,
    /// Output schema (optional)  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<SchemaRef>,
    /// Component description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComponentList {
    pub components: Vec<ComponentDetails>,
}

/// List all available components from a stepflow config.
pub async fn list_components(
    config_path: Option<PathBuf>,
    format: OutputFormat,
    schemas: Option<bool>,
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

    // Get all registered plugins and query their components
    let mut all_components = Vec::new();

    // Get the list of plugins from the executor
    for (_protocol, plugin) in executor.list_plugins().await {
        // List components available from this plugin
        let components = plugin
            .list_components()
            .await
            .change_context(MainError::PluginCommunication)?;

        // For each component, get detailed information
        for component in components {
            let info = plugin
                .component_info(&component)
                .await
                .change_context(MainError::PluginCommunication)?;

            let details = ComponentDetails {
                component,
                input_schema: if include_schemas {
                    Some(info.input_schema)
                } else {
                    None
                },
                output_schema: if include_schemas {
                    Some(info.output_schema)
                } else {
                    None
                },
                description: info.description,
            };

            all_components.push(details);
        }
    }

    // Sort components by their URL for consistent output
    all_components.sort_by(|a, b| a.component.to_string().cmp(&b.component.to_string()));

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

fn print_pretty(components: &[ComponentDetails]) {
    if components.is_empty() {
        println!("No components found.");
        return;
    }

    println!("Available Components:");
    println!("====================");

    for component in components {
        println!();
        println!("Component: {}", component.component);

        // Print description if present
        if let Some(ref description) = component.description {
            println!("  Description: {description}");
        }

        // Print input schema if present
        if let Some(ref input_schema) = component.input_schema {
            println!("  Input Schema:");
            let input_schema_str = serde_json::to_string_pretty(input_schema)
                .unwrap_or_else(|_| "Error serializing schema".to_string());
            for line in input_schema_str.lines() {
                println!("    {line}");
            }
        }

        // Print output schema if present
        if let Some(ref output_schema) = component.output_schema {
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
