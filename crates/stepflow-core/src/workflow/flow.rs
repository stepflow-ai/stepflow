use super::{Step, ValueRef, BaseRef, Expr, WorkflowGraph, StepPorts, Port, Edge, EdgeEndpoint};
use crate::{FlowResult, schema::SchemaRef};
use schemars::JsonSchema;

/// A workflow consisting of a sequence of steps and their outputs.
///
/// A flow represents a complete workflow that can be executed. It contains:
/// - A sequence of steps to execute
/// - Named outputs that can reference step outputs
///
/// Flows should not be cloned. They should generally be stored and passed as a
/// reference or inside an `Arc`.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Default, JsonSchema)]
pub struct Flow {
    /// The name of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The description of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The version of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// The input schema of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<SchemaRef>,

    /// The output schema of the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<SchemaRef>,

    /// The steps to execute for the flow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,

    /// The outputs of the flow, mapping output names to their values.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub output: serde_json::Value,

    /// Test configuration for the flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestConfig>,
}

impl Flow {
    /// Returns a reference to all steps in the flow.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Returns a reference to the step at the given index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    pub fn step(&self, index: usize) -> &Step {
        &self.steps[index]
    }

    /// Returns a reference to the flow's output value.
    pub fn output(&self) -> &serde_json::Value {
        &self.output
    }

    /// Parses a flow from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is invalid or cannot be deserialized into a Flow.
    pub fn from_yaml_string(yaml: &str) -> serde_yaml_ng::Result<Self> {
        serde_yaml_ng::from_str(yaml)
    }

    /// Parses a flow from a YAML reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML is invalid or cannot be deserialized into a Flow.
    pub fn from_yaml_reader(rdr: impl std::io::Read) -> serde_yaml_ng::Result<Self> {
        serde_yaml_ng::from_reader(rdr)
    }

    /// Parses a flow from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid or cannot be deserialized into a Flow.
    pub fn from_json_string(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Serializes the flow to a YAML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow cannot be serialized to YAML.
    pub fn to_yaml_string(&self) -> serde_yaml_ng::Result<String> {
        serde_yaml_ng::to_string(self)
    }

    /// Serializes the flow to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow cannot be serialized to JSON.
    pub fn to_json_string(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Analyze the workflow to extract ports and edges information.
    ///
    /// This method examines the step inputs, outputs, and dependencies to build
    /// an explicit graph representation of the workflow's data flow.
    pub fn analyze_ports_and_edges(&self) -> WorkflowGraph {
        let mut graph = WorkflowGraph::new();
        let mut edge_counter = 0;

        // Analyze workflow-level inputs and outputs
        graph = graph.with_workflow_ports(self.extract_workflow_ports());

        // Analyze each step to extract ports and edges
        for step in &self.steps {
            let step_ports = self.extract_step_ports(step);
            graph = graph.add_step_ports(step.id.clone(), step_ports);

            // Extract edges from step input expressions
            let edges = self.extract_step_input_edges(step, &mut edge_counter);
            for edge in edges {
                graph = graph.add_edge(edge);
            }

            // Extract edges from skip conditions
            if let Some(skip_if) = &step.skip_if {
                let edges = self.extract_expression_edges(skip_if, &step.id, "skip_condition", &mut edge_counter);
                for edge in edges {
                    graph = graph.add_edge(edge);
                }
            }
        }

        // Extract edges from workflow output expressions
        let output_edges = self.extract_workflow_output_edges(&mut edge_counter);
        for edge in output_edges {
            graph = graph.add_edge(edge);
        }

        graph
    }

    /// Extract workflow-level input and output ports
    fn extract_workflow_ports(&self) -> StepPorts {
        let mut ports = StepPorts::new();

        // Extract input ports from workflow input schema
        if let Some(input_schema) = &self.input_schema {
            // For now, create a generic "input" port
            // TODO: Parse schema to extract individual input fields as ports
            ports = ports.add_input(
                Port::new("input")
                    .with_schema(input_schema.clone())
                    .with_description("Workflow input data")
            );
        } else {
            // Default input port
            ports = ports.add_input(Port::new("input").with_description("Workflow input data"));
        }

        // Extract output ports from workflow output schema
        if let Some(output_schema) = &self.output_schema {
            ports = ports.add_output(
                Port::new("output")
                    .with_schema(output_schema.clone())
                    .with_description("Workflow output data")
            );
        } else {
            // Default output port
            ports = ports.add_output(Port::new("output").with_description("Workflow output data"));
        }

        ports
    }

    /// Extract input and output ports for a step
    fn extract_step_ports(&self, step: &Step) -> StepPorts {
        let mut ports = StepPorts::new();

        // Extract input ports from step input schema
        if let Some(input_schema) = &step.input_schema {
            ports = ports.add_input(
                Port::new("input")
                    .with_schema(input_schema.clone())
                    .with_description("Step input data")
            );
        } else {
            // Default input port
            ports = ports.add_input(Port::new("input").with_description("Step input data"));
        }

        // Extract output ports from step output schema
        if let Some(output_schema) = &step.output_schema {
            ports = ports.add_output(
                Port::new("output")
                    .with_schema(output_schema.clone())
                    .with_description("Step output data")
            );
        } else {
            // Default output port - most steps produce a "result"
            ports = ports.add_output(Port::new("result").with_description("Step result data"));
        }

        ports
    }

    /// Extract edges from step input expressions
    fn extract_step_input_edges(&self, step: &Step, edge_counter: &mut usize) -> Vec<Edge> {
        let mut edges = Vec::new();
        
        // Analyze the step's input ValueRef for expressions
        let step_input_edges = self.extract_value_ref_edges(&step.input, &step.id, "input", edge_counter);
        edges.extend(step_input_edges);

        edges
    }

    /// Extract edges from a ValueRef (recursive analysis)
    fn extract_value_ref_edges(&self, value_ref: &ValueRef, target_step_id: &str, target_port: &str, edge_counter: &mut usize) -> Vec<Edge> {
        let mut edges = Vec::new();
        
        // Check if this is an object with nested values
        if let Some(obj) = value_ref.as_object() {
            // Check if this object contains a $from field (indicating it's an expression)
            if obj.contains_key("$from") {
                if let Ok(expr) = value_ref.deserialize::<Expr>() {
                    let expr_edges = self.extract_expression_edges(&expr, target_step_id, target_port, edge_counter);
                    edges.extend(expr_edges);
                }
            } else {
                // Recursively analyze object fields
                for (field_name, field_value) in obj.iter() {
                    let field_edges = self.extract_value_ref_edges(&field_value, target_step_id, &format!("{}.{}", target_port, field_name), edge_counter);
                    edges.extend(field_edges);
                }
            }
        } else if let Some(arr) = value_ref.as_array() {
            // Recursively analyze array elements
            for (index, element) in arr.iter().enumerate() {
                let element_edges = self.extract_value_ref_edges(&element, target_step_id, &format!("{}[{}]", target_port, index), edge_counter);
                edges.extend(element_edges);
            }
        } else {
            // Try to parse the ValueRef as an Expr directly
            if let Ok(expr) = value_ref.deserialize::<Expr>() {
                let expr_edges = self.extract_expression_edges(&expr, target_step_id, target_port, edge_counter);
                edges.extend(expr_edges);
            }
        }
        
        edges
    }

    /// Extract edges from an expression
    fn extract_expression_edges(&self, expr: &Expr, target_step_id: &str, target_port: &str, edge_counter: &mut usize) -> Vec<Edge> {
        let mut edges = Vec::new();

        if let Expr::Ref { from, path, .. } = expr {
            let source_endpoint = match from {
                BaseRef::Workflow(_) => EdgeEndpoint::workflow_input("input"),
                BaseRef::Step { step } => EdgeEndpoint::new(step.clone(), "result"),
            };

            let target_endpoint = EdgeEndpoint::new(target_step_id, target_port);

            let edge_id = format!("edge_{}", edge_counter);
            *edge_counter += 1;

            let mut edge = Edge::new(edge_id, source_endpoint, target_endpoint);
            if let Some(path) = path {
                edge = edge.with_path(path.clone());
            }

            edges.push(edge);
        }

        edges
    }

    /// Extract edges from workflow output expressions
    fn extract_workflow_output_edges(&self, edge_counter: &mut usize) -> Vec<Edge> {
        let mut edges = Vec::new();

        // Convert the workflow output to a ValueRef and recursively extract edges
        let output_value_ref = ValueRef::new(self.output.clone());
        
        if let Some(obj) = output_value_ref.as_object() {
            for (output_name, output_value) in obj.iter() {
                let output_edges = self.extract_value_ref_edges(&output_value, "workflow", &format!("output.{}", output_name), edge_counter);
                edges.extend(output_edges);
            }
        } else {
            // If the output is not an object, treat it as a single output
            let output_edges = self.extract_value_ref_edges(&output_value_ref, "workflow", "output", edge_counter);
            edges.extend(output_edges);
        }

        edges
    }
}

/// A wrapper around Arc<Flow> to support poem-openapi traits.
///
/// This wrapper exists to work around Rust's orphan rules which prevent
/// implementing external traits on external types like Arc<Flow>.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowRef(std::sync::Arc<Flow>);

impl FlowRef {
    /// Create a new FlowRef from a Flow.
    pub fn new(flow: Flow) -> Self {
        Self(std::sync::Arc::new(flow))
    }

    /// Create a new FlowRef from an Arc<Flow>.
    pub fn from_arc(arc: std::sync::Arc<Flow>) -> Self {
        Self(arc)
    }

    /// Get a reference to the underlying Flow.
    pub fn as_flow(&self) -> &Flow {
        &self.0
    }

    /// Get the underlying Arc<Flow>.
    pub fn into_arc(self) -> std::sync::Arc<Flow> {
        self.0
    }

    /// Get a reference to the underlying Arc<Flow>.
    pub fn as_arc(&self) -> &std::sync::Arc<Flow> {
        &self.0
    }
}

impl std::ops::Deref for FlowRef {
    type Target = Flow;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Flow> for FlowRef {
    fn from(flow: Flow) -> Self {
        Self::new(flow)
    }
}

impl From<std::sync::Arc<Flow>> for FlowRef {
    fn from(arc: std::sync::Arc<Flow>) -> Self {
        Self::from_arc(arc)
    }
}

impl serde::Serialize for FlowRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for FlowRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let flow = Flow::deserialize(deserializer)?;
        Ok(Self::new(flow))
    }
}

impl JsonSchema for FlowRef {
    fn schema_name() -> String {
        Flow::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::schema::Schema {
        Flow::json_schema(generator)
    }

    fn is_referenceable() -> bool {
        Flow::is_referenceable()
    }
}

#[cfg(test)]
mod tests {
    use crate::workflow::{Component, ErrorAction};

    use super::*;

    #[test]
    fn test_analyze_ports_and_edges() {
        let yaml = r#"
        name: test
        description: test
        input_schema:
            type: object
            properties:
                name:
                    type: string
                count:
                    type: integer
        steps:
          - component: test://component
            id: step1
            input:
              name: { $from: { workflow: input }, path: "name" }
              count: { $from: { workflow: input }, path: "count" }
          - component: test://component2
            id: step2
            input:
              result: { $from: { step: step1 }, path: "output" }
        output:
            step1_result: { $from: { step: step1 }, path: "output" }
            step2_result: { $from: { step: step2 }, path: "output" }
        "#;
        
        let flow: Flow = serde_yaml_ng::from_str(yaml).unwrap();
        let graph = flow.analyze_ports_and_edges();
        
        // Check workflow ports
        assert_eq!(graph.workflow_ports.inputs.len(), 1);
        assert_eq!(graph.workflow_ports.outputs.len(), 1);
        assert_eq!(graph.workflow_ports.inputs[0].name, "input");
        assert_eq!(graph.workflow_ports.outputs[0].name, "output");
        
        // Check step ports
        assert_eq!(graph.step_ports.len(), 2);
        assert!(graph.step_ports.contains_key("step1"));
        assert!(graph.step_ports.contains_key("step2"));
        
        // Check that we have the expected number of edges
        assert_eq!(graph.edges.len(), 5, "Expected 5 edges: 2 workflow inputs, 1 step-to-step, 2 workflow outputs");
        
        // Find incoming edges for step1's name input
        let step1_name_incoming = graph.find_incoming_edges("step1", "input.name");
        assert_eq!(step1_name_incoming.len(), 1, "Expected 1 edge for step1:input.name");
        assert_eq!(step1_name_incoming[0].source.step_id, "workflow");
        assert_eq!(step1_name_incoming[0].source.port_name, "input");
        
        // Find incoming edges for step2's result input
        let step2_result_incoming = graph.find_incoming_edges("step2", "input.result");
        assert_eq!(step2_result_incoming.len(), 1, "Expected 1 edge for step2:input.result");
        assert_eq!(step2_result_incoming[0].source.step_id, "step1");
        assert_eq!(step2_result_incoming[0].source.port_name, "result");
        
        // Find outgoing edges from step1
        let step1_outgoing = graph.find_outgoing_edges("step1", "result");
        assert_eq!(step1_outgoing.len(), 2, "Expected 2 edges from step1:result (to step2 and workflow output)");
    }

    #[test]
    fn test_flow_from_yaml() {
        let yaml = r#"
        name: test
        description: test
        version: 1.0.0
        input_schema:
            type: object
            properties:
                name:
                    type: string
                    description: The name to echo
                count:
                    type: integer
        steps:
          - component: langflow://echo
            id: s1
            input:
              a: "hello world"
          - component: mcp+http://foo/bar
            id: s2
            input:
              a: "hello world 2"
        output:
            s1a: { $from: { step: s1 }, path: "a" }
            s2b: { $from: { step: s2 }, path: a }
        output_schema:
            type: object
            properties:
                s1a:
                    type: string
                s2b:
                    type: string
        "#;
        let flow: Flow = serde_yaml_ng::from_str(yaml).unwrap();
        let input_schema = SchemaRef::parse_json(r#"{"type":"object","properties":{"name":{"type":"string","description":"The name to echo"},"count":{"type":"integer"}}}"#).unwrap();
        let output_schema = SchemaRef::parse_json(
            r#"{"type":"object","properties":{"s1a":{"type":"string"},"s2b":{"type":"string"}}}"#,
        )
        .unwrap();
        similar_asserts::assert_serde_eq!(
            flow,
            Flow {
                name: Some("test".to_owned()),
                description: Some("test".to_owned()),
                version: Some("1.0.0".to_owned()),
                input_schema: Some(input_schema),
                steps: vec![
                    Step {
                        id: "s1".to_owned(),
                        component: Component::parse("langflow://echo").unwrap(),
                        input: serde_json::json!({
                            "a": "hello world"
                        })
                        .into(),
                        input_schema: None,
                        output_schema: None,
                        skip_if: None,
                        on_error: ErrorAction::default(),
                    },
                    Step {
                        id: "s2".to_owned(),
                        component: Component::parse("mcp+http://foo/bar").unwrap(),
                        input: serde_json::json!({
                            "a": "hello world 2"
                        })
                        .into(),
                        input_schema: None,
                        output_schema: None,
                        skip_if: None,
                        on_error: ErrorAction::default(),
                    }
                ],
                output: serde_json::json!({
                    "s1a": { "$from": { "step": "s1" }, "path": "a" },
                    "s2b": { "$from": { "step": "s2" }, "path": "a" }
                }),
                output_schema: Some(output_schema),
                test: None,
            }
        );
    }
}

/// Configuration for testing a workflow.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema)]
pub struct TestConfig {
    /// Stepflow configuration specific to tests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepflow_config: Option<serde_json::Value>,

    /// Test cases for the workflow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cases: Vec<TestCase>,
}

/// A single test case for a workflow.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema)]
pub struct TestCase {
    /// Unique identifier for the test case.
    pub name: String,

    /// Optional description of what this test case verifies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Input data for the workflow in this test case.
    pub input: ValueRef,

    /// Expected output from the workflow for this test case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<FlowResult>,
}
