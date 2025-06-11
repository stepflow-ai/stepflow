use std::collections::HashMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::schema::SchemaRef;

/// A port represents an input or output interface for a step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Port {
    /// Port identifier within the step
    pub name: String,
    /// Data type/schema information for this port
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaRef>,
    /// Whether this port is required for execution
    #[serde(default)]
    pub required: bool,
    /// Human-readable description of the port
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Port {
    /// Create a new port with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
            required: false,
            description: None,
        }
    }

    /// Set this port as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set the schema for this port
    pub fn with_schema(mut self, schema: SchemaRef) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set the description for this port
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Input and output ports for a step
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StepPorts {
    /// Input ports for this step
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Port>,
    /// Output ports for this step
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<Port>,
}

impl StepPorts {
    /// Create empty step ports
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Add an input port
    pub fn add_input(mut self, port: Port) -> Self {
        self.inputs.push(port);
        self
    }

    /// Add an output port
    pub fn add_output(mut self, port: Port) -> Self {
        self.outputs.push(port);
        self
    }

    /// Find an input port by name
    pub fn find_input(&self, name: &str) -> Option<&Port> {
        self.inputs.iter().find(|p| p.name == name)
    }

    /// Find an output port by name
    pub fn find_output(&self, name: &str) -> Option<&Port> {
        self.outputs.iter().find(|p| p.name == name)
    }
}

impl Default for StepPorts {
    fn default() -> Self {
        Self::new()
    }
}

/// An edge endpoint representing one side of a data connection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EdgeEndpoint {
    /// Step ID (or "workflow" for workflow input/output)
    pub step_id: String,
    /// Port name within the step
    pub port_name: String,
}

impl EdgeEndpoint {
    /// Create a new edge endpoint
    pub fn new(step_id: impl Into<String>, port_name: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
            port_name: port_name.into(),
        }
    }

    /// Create an endpoint for workflow input
    pub fn workflow_input(port_name: impl Into<String>) -> Self {
        Self::new("workflow", port_name)
    }

    /// Create an endpoint for workflow output
    pub fn workflow_output(port_name: impl Into<String>) -> Self {
        Self::new("workflow", port_name)
    }

    /// Check if this endpoint is for the workflow
    pub fn is_workflow(&self) -> bool {
        self.step_id == "workflow"
    }
}

/// An edge represents a data flow connection between two ports
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Edge {
    /// Unique identifier for this edge
    pub id: String,
    /// Source step ID and output port
    pub source: EdgeEndpoint,
    /// Target step ID and input port
    pub target: EdgeEndpoint,
    /// JSONPath for data transformation (corresponds to `path` in expressions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Human-readable description of this edge
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Edge {
    /// Create a new edge with the given ID and endpoints
    pub fn new(
        id: impl Into<String>,
        source: EdgeEndpoint,
        target: EdgeEndpoint,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            target,
            path: None,
            description: None,
        }
    }

    /// Set the JSONPath for this edge
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the description for this edge
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Complete ports and edges information for a workflow
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowGraph {
    /// Ports for the workflow itself (input/output)
    pub workflow_ports: StepPorts,
    /// Ports for each step, keyed by step ID
    pub step_ports: HashMap<String, StepPorts>,
    /// All edges in the workflow
    pub edges: Vec<Edge>,
}

impl WorkflowGraph {
    /// Create a new empty workflow graph
    pub fn new() -> Self {
        Self {
            workflow_ports: StepPorts::new(),
            step_ports: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Set the workflow ports
    pub fn with_workflow_ports(mut self, ports: StepPorts) -> Self {
        self.workflow_ports = ports;
        self
    }

    /// Add ports for a step
    pub fn add_step_ports(mut self, step_id: impl Into<String>, ports: StepPorts) -> Self {
        self.step_ports.insert(step_id.into(), ports);
        self
    }

    /// Add an edge
    pub fn add_edge(mut self, edge: Edge) -> Self {
        self.edges.push(edge);
        self
    }

    /// Get ports for a specific step
    pub fn get_step_ports(&self, step_id: &str) -> Option<&StepPorts> {
        self.step_ports.get(step_id)
    }

    /// Find all edges targeting a specific step and port
    pub fn find_incoming_edges(&self, step_id: &str, port_name: &str) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|edge| edge.target.step_id == step_id && edge.target.port_name == port_name)
            .collect()
    }

    /// Find all edges originating from a specific step and port
    pub fn find_outgoing_edges(&self, step_id: &str, port_name: &str) -> Vec<&Edge> {
        self.edges
            .iter()
            .filter(|edge| edge.source.step_id == step_id && edge.source.port_name == port_name)
            .collect()
    }
}

impl Default for WorkflowGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_creation() {
        let port = Port::new("input_data")
            .required()
            .with_description("Main input data");

        assert_eq!(port.name, "input_data");
        assert!(port.required);
        assert_eq!(port.description, Some("Main input data".to_string()));
    }

    #[test]
    fn test_step_ports() {
        let ports = StepPorts::new()
            .add_input(Port::new("data").required())
            .add_output(Port::new("result"));

        assert_eq!(ports.inputs.len(), 1);
        assert_eq!(ports.outputs.len(), 1);
        
        assert!(ports.find_input("data").is_some());
        assert!(ports.find_input("missing").is_none());
        assert!(ports.find_output("result").is_some());
    }

    #[test]
    fn test_edge_creation() {
        let source = EdgeEndpoint::new("step1", "output");
        let target = EdgeEndpoint::new("step2", "input");
        
        let edge = Edge::new("edge1", source, target)
            .with_path("result")
            .with_description("Data flow from step1 to step2");

        assert_eq!(edge.id, "edge1");
        assert_eq!(edge.source.step_id, "step1");
        assert_eq!(edge.target.step_id, "step2");
        assert_eq!(edge.path, Some("result".to_string()));
    }

    #[test]
    fn test_workflow_input_endpoint() {
        let endpoint = EdgeEndpoint::workflow_input("sales_data");
        
        assert_eq!(endpoint.step_id, "workflow");
        assert_eq!(endpoint.port_name, "sales_data");
        assert!(endpoint.is_workflow());
    }

    #[test]
    fn test_workflow_graph() {
        let mut graph = WorkflowGraph::new();
        
        // Add workflow ports
        graph = graph.with_workflow_ports(
            StepPorts::new()
                .add_input(Port::new("sales_data"))
                .add_output(Port::new("insights"))
        );

        // Add step ports
        graph = graph.add_step_ports("process", 
            StepPorts::new()
                .add_input(Port::new("data"))
                .add_output(Port::new("result"))
        );

        // Add edge
        let edge = Edge::new(
            "wf_to_process",
            EdgeEndpoint::workflow_input("sales_data"),
            EdgeEndpoint::new("process", "data")
        );
        graph = graph.add_edge(edge);

        assert_eq!(graph.step_ports.len(), 1);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.workflow_ports.inputs.len(), 1);
    }

    #[test]
    fn test_edge_queries() {
        let graph = WorkflowGraph::new()
            .add_edge(Edge::new(
                "edge1",
                EdgeEndpoint::new("step1", "out"),
                EdgeEndpoint::new("step2", "in")
            ))
            .add_edge(Edge::new(
                "edge2", 
                EdgeEndpoint::new("step2", "out"),
                EdgeEndpoint::new("step3", "in")
            ));

        let incoming = graph.find_incoming_edges("step2", "in");
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].id, "edge1");

        let outgoing = graph.find_outgoing_edges("step2", "out");
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].id, "edge2");
    }
}