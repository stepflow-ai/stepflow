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

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use error_stack::{ResultExt as _, report};
use stepflow_analysis::{Dependency, FlowAnalysis, validate_and_analyze};
use stepflow_core::{ValueExpr, workflow::Flow};
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

impl OutputFormat {
    /// Get the file extension for this format (without the leading dot)
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Dot => "dot",
            OutputFormat::Svg => "svg",
            OutputFormat::Png => "png",
        }
    }
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
    expr_node_counter: usize,
}

impl FlowVisualizer {
    pub fn new(flow: std::sync::Arc<Flow>, router: Option<PluginRouter>) -> Self {
        Self {
            flow,
            router,
            config: VisualizationConfig::default(),
            expr_node_counter: 0,
        }
    }

    pub fn with_config(mut self, config: VisualizationConfig) -> Self {
        self.config = config;
        self
    }

    /// Generate visualization and write to the specified output path
    pub async fn generate(&mut self, output_path: &Path) -> Result<()> {
        let dot_content = self.generate_dot()?;

        match self.config.format {
            OutputFormat::Dot => {
                std::fs::write(output_path, dot_content).change_context(MainError::internal(
                    format!("Failed to write DOT file: {}", output_path.display()),
                ))?;
            }
            format => {
                self.render_graphviz(&dot_content, output_path, format)
                    .await?;
            }
        }

        Ok(())
    }

    /// Generate DOT format representation of the workflow
    pub fn generate_dot(&mut self) -> Result<String> {
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

        let analysis_result = validate_and_analyze(self.flow.clone(), blob_id)
            .change_context(MainError::internal("Failed to analyze flow dependencies"))?;

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
            log::warn!(
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

    /// Generate a unique ID for an expression node
    fn next_expr_id(&mut self) -> String {
        let id = self.expr_node_counter;
        self.expr_node_counter += 1;
        format!("expr_{}", id)
    }

    /// Add expression nodes and edges for a ValueExpr tree.
    /// Collects edges to be added to the graph with path labels.
    /// Returns a list of (source_node, label) pairs representing dependencies.
    fn collect_expr_edges(
        &mut self,
        dot: &mut String,
        expr: &ValueExpr,
        path: &str,
    ) -> Result<Vec<(String, String)>> {
        match expr {
            ValueExpr::If {
                condition,
                then,
                else_expr,
            } => {
                // Create diamond node for the $if expression
                let if_id = self.next_expr_id();
                writeln!(dot, "  \"{}\" [", if_id).unwrap();
                writeln!(dot, "    label=\"$if\",").unwrap();
                writeln!(dot, "    shape=diamond,").unwrap();
                writeln!(dot, "    fillcolor=\"lightyellow\",").unwrap();
                writeln!(dot, "  ];").unwrap();
                writeln!(dot).unwrap();

                // Add edges from sub-expressions to the $if node with semantic labels
                for (source, label) in self.collect_expr_edges(dot, condition, "")? {
                    writeln!(dot, "  \"{}\" -> \"{}\" [", source, if_id).unwrap();
                    writeln!(
                        dot,
                        "    label=\"cond{}\",",
                        if label.is_empty() {
                            "".to_string()
                        } else {
                            format!(":{}", label)
                        }
                    )
                    .unwrap();
                    writeln!(dot, "  ];").unwrap();
                }

                for (source, label) in self.collect_expr_edges(dot, then, "")? {
                    writeln!(dot, "  \"{}\" -> \"{}\" [", source, if_id).unwrap();
                    writeln!(
                        dot,
                        "    label=\"then{}\",",
                        if label.is_empty() {
                            "".to_string()
                        } else {
                            format!(":{}", label)
                        }
                    )
                    .unwrap();
                    writeln!(dot, "  ];").unwrap();
                }

                if let Some(else_val) = else_expr {
                    for (source, label) in self.collect_expr_edges(dot, else_val, "")? {
                        writeln!(dot, "  \"{}\" -> \"{}\" [", source, if_id).unwrap();
                        writeln!(
                            dot,
                            "    label=\"else{}\",",
                            if label.is_empty() {
                                "".to_string()
                            } else {
                                format!(":{}", label)
                            }
                        )
                        .unwrap();
                        writeln!(dot, "  ];").unwrap();
                    }
                }

                writeln!(dot).unwrap();
                // The $if node itself becomes the source for the parent
                Ok(vec![(if_id, path.to_string())])
            }
            ValueExpr::Coalesce { values } => {
                // Create diamond node for the $coalesce expression
                let coalesce_id = self.next_expr_id();
                writeln!(dot, "  \"{}\" [", coalesce_id).unwrap();
                writeln!(dot, "    label=\"$coalesce\",").unwrap();
                writeln!(dot, "    shape=diamond,").unwrap();
                writeln!(dot, "    fillcolor=\"lightcyan\",").unwrap();
                writeln!(dot, "  ];").unwrap();
                writeln!(dot).unwrap();

                // Add edges from each value to the $coalesce node
                for (idx, value_expr) in values.iter().enumerate() {
                    for (source, label) in self.collect_expr_edges(dot, value_expr, "")? {
                        let idx_label = if label.is_empty() {
                            format!("{}", idx)
                        } else {
                            format!("{}:{}", idx, label)
                        };
                        writeln!(dot, "  \"{}\" -> \"{}\" [", source, coalesce_id).unwrap();
                        writeln!(dot, "    label=\"{}\",", idx_label).unwrap();
                        writeln!(dot, "  ];").unwrap();
                    }
                }

                writeln!(dot).unwrap();
                // The $coalesce node itself becomes the source for the parent
                Ok(vec![(coalesce_id, path.to_string())])
            }
            ValueExpr::Step { step, .. } => {
                // Return the step ID as a dependency
                Ok(vec![(step.clone(), path.to_string())])
            }
            ValueExpr::Input { .. } => {
                // Return workflow_input as a dependency
                Ok(vec![("workflow_input".to_string(), path.to_string())])
            }
            ValueExpr::Array(items) => {
                // Collect edges from all array items with index paths
                let mut edges = Vec::new();
                for (idx, item) in items.iter().enumerate() {
                    let item_path = if path.is_empty() {
                        format!("[{}]", idx)
                    } else {
                        format!("{}[{}]", path, idx)
                    };
                    edges.extend(self.collect_expr_edges(dot, item, &item_path)?);
                }
                Ok(edges)
            }
            ValueExpr::Object(fields) => {
                // Collect edges from all object fields with field paths
                let mut edges = Vec::new();
                for (key, value) in fields.iter() {
                    let field_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    edges.extend(self.collect_expr_edges(dot, value, &field_path)?);
                }
                Ok(edges)
            }
            ValueExpr::Literal(_)
            | ValueExpr::EscapedLiteral { .. }
            | ValueExpr::Variable { .. } => {
                // Literals and variables have no step/input dependencies to visualize
                Ok(vec![])
            }
        }
    }

    fn add_edges(&mut self, dot: &mut String, analysis: &FlowAnalysis) -> Result<()> {
        let num_steps = self.flow.steps().len();
        for step_idx in 0..num_steps {
            let step = &self.flow.steps()[step_idx];
            let step_id = step.id.clone();
            let step_input = step.input.clone();
            let step_skip_if = step.skip_if.clone();

            // Collect edges from the step input expression
            let edges = self.collect_expr_edges(dot, &step_input, "")?;

            // Add edges from dependencies to the step with path labels (at tail/source)
            for (source, label) in edges {
                if source != step_id {
                    writeln!(dot, "  \"{}\" -> \"{}\" [", source, step_id).unwrap();
                    if !label.is_empty() {
                        writeln!(dot, "    label=\"{}\",", label).unwrap();
                    }
                    writeln!(dot, "  ];").unwrap();
                }
            }

            // Add edges for skip conditions
            if let Some(skip_if) = &step_skip_if {
                let skip_edges = self.collect_expr_edges(dot, skip_if, "")?;
                for (source, _label) in skip_edges {
                    writeln!(dot, "  \"{}\" -> \"{}\" [", source, step_id).unwrap();
                    writeln!(dot, "    style=\"dotted\",").unwrap();
                    writeln!(dot, "    label=\"skipIf\",").unwrap();
                    writeln!(dot, "  ];").unwrap();
                }
            }

            // Also check analysis for skip_if_depends (from analysis)
            if step_skip_if.is_none()
                && let Some(step_analysis) = analysis.steps.get(&step_id)
            {
                for skip_dep in &step_analysis.skip_if_depends {
                    let source = match skip_dep {
                        Dependency::FlowInput { .. } => "workflow_input".to_string(),
                        Dependency::StepOutput { step_id, .. } => step_id.clone(),
                    };
                    writeln!(dot, "  \"{}\" -> \"{}\" [", source, step_id).unwrap();
                    writeln!(dot, "    style=\"dotted\",").unwrap();
                    writeln!(dot, "    label=\"skipIf\",").unwrap();
                    writeln!(dot, "  ];").unwrap();
                }
            }
        }

        Ok(())
    }

    fn add_output_edges(&mut self, dot: &mut String, _analysis: &FlowAnalysis) -> Result<()> {
        // Collect edges from the workflow output expression
        let output = self.flow.output().clone();
        let edges = self.collect_expr_edges(dot, &output, "")?;

        // Add edges from dependencies to workflow_output with path labels (at tail/source)
        for (source, label) in edges {
            writeln!(dot, "  \"{}\" -> \"workflow_output\" [", source).unwrap();
            if !label.is_empty() {
                writeln!(dot, "    label=\"{}\",", label).unwrap();
            }
            writeln!(dot, "  ];").unwrap();
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

        match step.on_error_or_default() {
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
        use tokio::io::AsyncWriteExt as _;

        let dot = which::which("dot").change_context(MainError::internal(
            "Graphviz 'dot' command not found. Please install Graphviz to use this feature.",
        ))?;

        log::debug!(
            "Running graphviz: {:?} -T{} -o {:?}",
            dot,
            format,
            output_path
        );

        let mut cmd = tokio::process::Command::new(dot);
        cmd.arg(format!("-T{}", format))
            .arg("-o")
            .arg(output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().change_context(MainError::internal(
            "Failed to spawn graphviz 'dot' command",
        ))?;

        // Write DOT content to stdin and ensure it's flushed and closed
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(dot_content.as_bytes())
                .await
                .change_context(MainError::internal(
                    "Failed to write DOT content to graphviz",
                ))?;
            stdin.flush().await.change_context(MainError::internal(
                "Failed to flush DOT content to graphviz",
            ))?;
            // stdin is dropped here, closing the pipe
        }

        let output = child
            .wait_with_output()
            .await
            .change_context(MainError::internal("Failed to wait for graphviz child"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(report!(MainError::internal(format!(
                "Graphviz rendering failed: {}",
                stderr
            ))));
        }

        // Verify the output file was created
        if !output_path.exists() {
            return Err(report!(MainError::internal(format!(
                "Graphviz completed but output file was not created: {}",
                output_path.display()
            ))));
        }

        log::debug!("Graphviz output written to: {:?}", output_path);

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
    let mut visualizer = FlowVisualizer::new(flow, router).with_config(config);
    visualizer.generate(output_path).await
}

/// Generate DOT visualization to stdout
#[allow(clippy::print_stdout)]
pub async fn visualize_flow_to_stdout(
    flow: std::sync::Arc<Flow>,
    router: Option<PluginRouter>,
    config: VisualizationConfig,
) -> Result<()> {
    let mut visualizer = FlowVisualizer::new(flow, router).with_config(config);
    let dot_content = visualizer.generate_dot()?;

    // Output to stdout

    println!("{}", dot_content);
    Ok(())
}
