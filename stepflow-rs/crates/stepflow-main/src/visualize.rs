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

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use error_stack::{ResultExt as _, report};
use stepflow_analysis::{Dependency, FlowAnalysis, analyze_flow_dependencies};
use stepflow_core::workflow::Flow;
use stepflow_plugin::routing::PluginRouter;

use crate::error::{MainError, Result};

/// Output formats supported for visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::Display, clap::ValueEnum)]
#[strum(serialize_all = "lowercase")]
pub enum OutputFormat {
    /// DOT graph format
    Dot,
    /// SVG image format
    Svg,
    /// PNG image format
    Png,
}

/// Configuration for visualization rendering
pub struct VisualizationConfig {
    /// Output format
    pub format: OutputFormat,
    /// Whether to include component server information
    pub show_component_servers: bool,
    /// Whether to show detailed step metadata
    pub show_details: bool,
    // Note: subflow rendering not yet implemented
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Svg,
            show_component_servers: true,
            show_details: true,
        }
    }
}

/// Visualization generator for Stepflow workflows
pub struct FlowVisualizer {
    flow: std::sync::Arc<Flow>,
    router: Option<PluginRouter>,
    config: VisualizationConfig,
}

impl FlowVisualizer {
    pub fn new(flow: std::sync::Arc<Flow>, router: Option<PluginRouter>) -> Self {
        Self {
            flow,
            router,
            config: VisualizationConfig::default(),
        }
    }

    pub fn with_config(mut self, config: VisualizationConfig) -> Self {
        self.config = config;
        self
    }

    /// Generate visualization and write to the specified output path
    pub async fn generate(&self, output_path: &Path) -> Result<()> {
        let dot_content = self.generate_dot()?;

        match self.config.format {
            OutputFormat::Dot => {
                std::fs::write(output_path, dot_content)
                    .change_context(MainError::Configuration)?;
            }
            format => {
                self.render_graphviz(&dot_content, output_path, format)
                    .await?;
            }
        }

        Ok(())
    }

    /// Generate DOT format representation of the workflow
    pub fn generate_dot(&self) -> Result<String> {
        let mut dot = String::new();

        // Start the digraph
        writeln!(&mut dot, "digraph workflow {{").unwrap();
        writeln!(&mut dot, "  rankdir=TB;").unwrap();
        writeln!(&mut dot, "  node [shape=box, style=filled];").unwrap();
        writeln!(&mut dot, "  edge [fontsize=10];").unwrap();
        writeln!(&mut dot).unwrap();

        // Analyze the entire workflow to get dependencies
        let blob_id = stepflow_core::BlobId::new(
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        )
        .unwrap();
        let analysis_result = analyze_flow_dependencies(self.flow.clone(), blob_id)
            .change_context(MainError::Configuration)?;

        // Create color mapping for component servers
        let server_colors = self.create_server_color_mapping();

        // Add workflow input node
        writeln!(&mut dot, "  \"workflow_input\" [").unwrap();
        writeln!(&mut dot, "    label=\"📥 Workflow Input\",").unwrap();
        writeln!(&mut dot, "    fillcolor=\"lightblue\",").unwrap();
        writeln!(&mut dot, "    shape=ellipse,").unwrap();
        if self.config.show_details {
            let tooltip = self.create_input_tooltip();
            writeln!(&mut dot, "    tooltip=\"{}\",", escape_for_dot(&tooltip)).unwrap();
        }
        writeln!(&mut dot, "  ];").unwrap();
        writeln!(&mut dot).unwrap();

        // Add step nodes
        for (step_index, step) in self.flow.steps().iter().enumerate() {
            self.add_step_node(&mut dot, step_index, step, &server_colors)?;
        }

        // Add workflow output node
        writeln!(&mut dot, "  \"workflow_output\" [").unwrap();
        writeln!(&mut dot, "    label=\"📤 Workflow Output\",").unwrap();
        writeln!(&mut dot, "    fillcolor=\"lightgreen\",").unwrap();
        writeln!(&mut dot, "    shape=ellipse,").unwrap();
        if self.config.show_details {
            let tooltip = self.create_output_tooltip();
            writeln!(&mut dot, "    tooltip=\"{}\",", escape_for_dot(&tooltip)).unwrap();
        }
        writeln!(&mut dot, "  ];").unwrap();
        writeln!(&mut dot).unwrap();

        // Add edges based on dependencies
        if let Some(analysis) = analysis_result.analysis.as_ref() {
            self.add_edges(&mut dot, analysis)?;
        } else {
            // If analysis failed due to validation errors, we can still show the basic structure
            tracing::warn!(
                "Workflow analysis failed, showing basic structure without detailed dependencies"
            );
        }

        // Add edges from steps to workflow output
        if let Some(analysis) = analysis_result.analysis.as_ref() {
            self.add_output_edges(&mut dot, analysis)?;
        }

        // End the digraph
        writeln!(&mut dot, "}}").unwrap();

        Ok(dot)
    }

    fn create_server_color_mapping(&self) -> HashMap<String, &'static str> {
        let mut colors = HashMap::new();
        let color_palette = [
            "lightcoral",
            "lightblue",
            "lightgreen",
            "lightyellow",
            "lightpink",
            "lightgray",
            "lightcyan",
            "lightsalmon",
            "lightsteelblue",
            "lightseagreen",
        ];

        let mut color_index = 0;
        for step in self.flow.steps() {
            let server_name = self.extract_server_name(step.component.path());
            if let std::collections::hash_map::Entry::Vacant(e) = colors.entry(server_name) {
                e.insert(color_palette[color_index % color_palette.len()]);
                color_index += 1;
            }
        }

        colors
    }

    fn extract_server_name(&self, component_path: &str) -> String {
        if let Some(_router) = &self.router {
            // Try to determine which plugin this component would route to
            // This is a simplified approach - in practice you'd need to evaluate routing rules
            if component_path.starts_with("/builtin/") || !component_path.starts_with('/') {
                "builtin".to_string()
            } else {
                // Extract the first path segment as the likely plugin name
                component_path
                    .strip_prefix('/')
                    .unwrap_or(component_path)
                    .split('/')
                    .next()
                    .unwrap_or("unknown")
                    .to_string()
            }
        } else {
            // Without router information, make best guess from component path
            if component_path.starts_with("/builtin/") || !component_path.starts_with('/') {
                "builtin".to_string()
            } else {
                component_path
                    .strip_prefix('/')
                    .unwrap_or(component_path)
                    .split('/')
                    .next()
                    .unwrap_or("unknown")
                    .to_string()
            }
        }
    }

    fn add_step_node(
        &self,
        dot: &mut String,
        step_index: usize,
        step: &stepflow_core::workflow::Step,
        server_colors: &HashMap<String, &'static str>,
    ) -> Result<()> {
        let server_name = self.extract_server_name(step.component.path());
        let color = server_colors.get(&server_name).unwrap_or(&"white");

        // Create the step label
        let mut label = format!("🔧 {}", step.id);
        if self.config.show_component_servers {
            label.push_str(&format!("\\n📦 {}", step.component.path()));
            label.push_str(&format!("\\n🖥️ {}", server_name));
        }

        writeln!(dot, "  \"{}\" [", step.id).unwrap();
        writeln!(dot, "    label=\"{}\",", label).unwrap();
        writeln!(dot, "    fillcolor=\"{}\",", color).unwrap();

        if self.config.show_details {
            let tooltip = self.create_step_tooltip(step_index, step);
            writeln!(dot, "    tooltip=\"{}\",", escape_for_dot(&tooltip)).unwrap();
        }

        writeln!(dot, "  ];").unwrap();

        Ok(())
    }

    fn add_edges(&self, dot: &mut String, analysis: &FlowAnalysis) -> Result<()> {
        for step in self.flow.steps().iter() {
            // Get the step analysis from the flow analysis
            if let Some(step_analysis) = analysis.steps.get(&step.id) {
                // Add edges based on input dependencies
                for dep in step_analysis.input_depends.dependencies() {
                    match dep {
                        Dependency::FlowInput { .. } => {
                            // Edge from workflow input to this step
                            writeln!(dot, "  \"workflow_input\" -> \"{}\" [", step.id).unwrap();
                            writeln!(dot, "    color=\"blue\",").unwrap();
                            writeln!(dot, "    style=\"solid\",").unwrap();
                            writeln!(dot, "  ];").unwrap();
                        }
                        Dependency::StepOutput {
                            step_id, optional, ..
                        } => {
                            // Edge from dependency step to this step
                            writeln!(dot, "  \"{}\" -> \"{}\" [", step_id, step.id).unwrap();
                            if *optional {
                                writeln!(dot, "    color=\"orange\",").unwrap();
                                writeln!(dot, "    style=\"dashed\",").unwrap();
                                writeln!(dot, "    label=\"optional\",").unwrap();
                            } else {
                                writeln!(dot, "    color=\"black\",").unwrap();
                                writeln!(dot, "    style=\"solid\",").unwrap();
                            }
                            writeln!(dot, "  ];").unwrap();
                        }
                    }
                }

                // Add edges for skip condition dependencies
                if let Some(skip_dep) = &step_analysis.skip_if_depend {
                    match skip_dep {
                        Dependency::FlowInput { .. } => {
                            writeln!(dot, "  \"workflow_input\" -> \"{}\" [", step.id).unwrap();
                            writeln!(dot, "    color=\"purple\",").unwrap();
                            writeln!(dot, "    style=\"dotted\",").unwrap();
                            writeln!(dot, "    label=\"skip condition\",").unwrap();
                            writeln!(dot, "  ];").unwrap();
                        }
                        Dependency::StepOutput { step_id, .. } => {
                            writeln!(dot, "  \"{}\" -> \"{}\" [", step_id, step.id).unwrap();
                            writeln!(dot, "    color=\"purple\",").unwrap();
                            writeln!(dot, "    style=\"dotted\",").unwrap();
                            writeln!(dot, "    label=\"skip condition\",").unwrap();
                            writeln!(dot, "  ];").unwrap();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn add_output_edges(&self, dot: &mut String, analysis: &FlowAnalysis) -> Result<()> {
        // Use the output dependencies from the analysis
        for dep in analysis.output_depends.dependencies() {
            match dep {
                Dependency::StepOutput { step_id, .. } => {
                    writeln!(dot, "  \"{}\" -> \"workflow_output\" [", step_id).unwrap();
                    writeln!(dot, "    color=\"green\",").unwrap();
                    writeln!(dot, "    style=\"solid\",").unwrap();
                    writeln!(dot, "  ];").unwrap();
                }
                Dependency::FlowInput { .. } => {
                    // Direct workflow input to output passthrough
                    writeln!(dot, "  \"workflow_input\" -> \"workflow_output\" [").unwrap();
                    writeln!(dot, "    color=\"blue\",").unwrap();
                    writeln!(dot, "    style=\"dotted\",").unwrap();
                    writeln!(dot, "    label=\"passthrough\",").unwrap();
                    writeln!(dot, "  ];").unwrap();
                }
            }
        }

        Ok(())
    }

    fn create_input_tooltip(&self) -> String {
        format!(
            "Workflow: {}\\nInput Schema: Available",
            self.flow.name().unwrap_or("Unnamed")
        )
    }

    fn create_output_tooltip(&self) -> String {
        format!(
            "Workflow: {}\\nOutput Schema: Available",
            self.flow.name().unwrap_or("Unnamed")
        )
    }

    fn create_step_tooltip(
        &self,
        _step_index: usize,
        step: &stepflow_core::workflow::Step,
    ) -> String {
        let mut tooltip = format!("Step: {}\\nComponent: {}", step.id, step.component.path());

        if let Some(_skip_condition) = &step.skip_if {
            tooltip.push_str("\\nSkip Condition: Present");
        }

        match &step.on_error {
            stepflow_core::workflow::ErrorAction::Fail => {}
            stepflow_core::workflow::ErrorAction::Skip => {
                tooltip.push_str("\\nError Action: Skip");
            }
            stepflow_core::workflow::ErrorAction::UseDefault { .. } => {
                tooltip.push_str("\\nError Action: Use Default");
            }
            stepflow_core::workflow::ErrorAction::Retry => {
                tooltip.push_str("\\nError Action: Retry");
            }
        }

        tooltip
    }

    async fn render_graphviz(
        &self,
        dot_content: &str,
        output_path: &Path,
        format: OutputFormat,
    ) -> Result<()> {
        use std::process::Stdio;

        let mut cmd = tokio::process::Command::new("dot");
        cmd.arg(format!("-T{}", format))
            .arg("-o")
            .arg(output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .change_context(MainError::Configuration)
            .attach_printable(
                "Failed to spawn graphviz 'dot' command. Make sure Graphviz is installed.",
            )?;

        if let Some(stdin) = child.stdin.take() {
            tokio::io::AsyncWriteExt::write_all(
                &mut tokio::io::BufWriter::new(stdin),
                dot_content.as_bytes(),
            )
            .await
            .change_context(MainError::Configuration)?;
        }

        let output = child
            .wait_with_output()
            .await
            .change_context(MainError::Configuration)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(report!(MainError::Configuration)
                .attach_printable(format!("Graphviz rendering failed: {}", stderr)));
        }

        Ok(())
    }
}

/// Escape string for safe use in DOT format
fn escape_for_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Generate visualization for a workflow
pub async fn visualize_flow(
    flow: std::sync::Arc<Flow>,
    router: Option<PluginRouter>,
    output_path: &Path,
    config: VisualizationConfig,
) -> Result<()> {
    let visualizer = FlowVisualizer::new(flow, router).with_config(config);
    visualizer.generate(output_path).await
}

/// Generate DOT visualization to stdout
pub async fn visualize_flow_to_stdout(
    flow: std::sync::Arc<Flow>,
    router: Option<PluginRouter>,
    config: VisualizationConfig,
) -> Result<()> {
    let visualizer = FlowVisualizer::new(flow, router).with_config(config);
    let dot_content = visualizer.generate_dot()?;

    // Output to stdout
    println!("{}", dot_content);
    Ok(())
}
