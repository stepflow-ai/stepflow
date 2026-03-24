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

//! High-level gRPC client for the Stepflow orchestrator.

use std::collections::HashMap;

use tonic::transport::Channel;

use crate::error::{ClientError, ClientResult};
use crate::flow::Flow;

use stepflow_proto::{
    CreateRunRequest, GetRunEventsRequest, GetRunItemsRequest, GetRunRequest, HealthCheckRequest,
    ListRegisteredComponentsRequest, StoreFlowRequest,
    components_service_client::ComponentsServiceClient, flows_service_client::FlowsServiceClient,
    health_service_client::HealthServiceClient, runs_service_client::RunsServiceClient,
};

// ---------------------------------------------------------------------------
// Public return types
// ---------------------------------------------------------------------------

/// Run status returned by [`StepflowClient::get_run`] and [`StepflowClient::run`].
#[derive(Debug, Clone)]
pub struct RunStatus {
    /// The run's unique identifier.
    pub run_id: String,
    /// Numeric execution status (see `ExecutionStatus` proto enum).
    pub status: i32,
    /// Outputs for each item in the run.
    ///
    /// Populated by [`StepflowClient::run`] (synchronous execution).
    /// Empty when returned by [`StepflowClient::get_run`] — use
    /// [`StepflowClient::get_run_items`] to fetch outputs for a completed run.
    pub outputs: Vec<serde_json::Value>,
}

/// A single registered component returned by [`StepflowClient::list_components`].
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    /// Component path (e.g. `/builtin/openai`, `/python/my_func`).
    pub component: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// JSON Schema for the component's input, if schemas were requested.
    pub input_schema: Option<serde_json::Value>,
    /// JSON Schema for the component's output, if schemas were requested.
    pub output_schema: Option<serde_json::Value>,
}

/// Result of [`StepflowClient::list_components`].
#[derive(Debug, Clone)]
pub struct ListComponentsResult {
    /// All discovered components, sorted by path.
    pub components: Vec<ComponentInfo>,
    /// `true` if all plugins responded successfully.
    ///
    /// When `false`, check `failed_plugins` for plugins that could not be
    /// reached during discovery.
    pub complete: bool,
    /// `(plugin_name, error_message)` pairs for plugins that failed discovery.
    pub failed_plugins: Vec<(String, String)>,
}

/// A variable definition returned by [`StepflowClient::get_flow_variables`].
#[derive(Debug, Clone)]
pub struct FlowVariable {
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Default value for the variable.
    pub default_value: Option<serde_json::Value>,
    /// Whether the variable must be provided at run time.
    pub required: bool,
    /// JSON Schema for the variable's expected value.
    pub schema: Option<serde_json::Value>,
    /// Environment variable that populates this variable, if any.
    pub env_var: Option<String>,
}

// ---------------------------------------------------------------------------
// Type alias for the status event stream
// ---------------------------------------------------------------------------

/// A streaming response of [`stepflow_proto::StatusEvent`]s from [`StepflowClient::status_events`].
///
/// Drive the stream with [`futures::StreamExt::next`] or `while let Some(event) = stream.message().await`.
pub type StatusEventStream = tonic::codec::Streaming<stepflow_proto::StatusEvent>;

// ---------------------------------------------------------------------------
// StepflowClient
// ---------------------------------------------------------------------------

/// High-level client for interacting with the Stepflow orchestrator.
///
/// Wraps the gRPC service clients for flows, runs, health, and component
/// discovery, providing a convenient API for common operations.
///
/// # Example
///
/// ```rust,no_run
/// use stepflow_client::{StepflowClient, FlowBuilder, ValueExpr};
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = StepflowClient::connect("http://localhost:7840").await?;
///
/// let mut builder = FlowBuilder::new();
/// builder.add_step("hello", "/builtin/eval", ValueExpr::input(None));
/// let flow = builder.output(stepflow_client::FlowBuilder::step("hello")).build()?;
///
/// let flow_id = client.store_flow(&flow).await?;
/// let output = client.run(&flow_id, serde_json::json!({"name": "world"})).await?;
/// println!("{output}");
/// # Ok(())
/// # }
/// ```
pub struct StepflowClient {
    flows: FlowsServiceClient<Channel>,
    runs: RunsServiceClient<Channel>,
    health: HealthServiceClient<Channel>,
    components: ComponentsServiceClient<Channel>,
}

impl StepflowClient {
    /// Connect to the Stepflow orchestrator at the given URL.
    ///
    /// The URL should be in the form `http://host:port` (or `https://...` for TLS).
    pub async fn connect(url: impl Into<String>) -> ClientResult<Self> {
        let url = url.into();
        let channel = Channel::from_shared(url.clone())
            .map_err(|e| ClientError::Connection {
                url: url.clone(),
                source: Box::new(e),
            })?
            .connect()
            .await
            .map_err(|e| ClientError::Connection {
                url,
                source: Box::new(e),
            })?;

        Ok(Self {
            flows: FlowsServiceClient::new(channel.clone()),
            runs: RunsServiceClient::new(channel.clone()),
            health: HealthServiceClient::new(channel.clone()),
            components: ComponentsServiceClient::new(channel),
        })
    }

    /// Store a flow definition in the orchestrator, returning its flow ID.
    ///
    /// The returned flow ID can be passed to [`run`](Self::run) or
    /// [`submit`](Self::submit).
    pub async fn store_flow(&mut self, flow: &Flow) -> ClientResult<String> {
        let flow_json = flow.to_json()?;
        let flow_value = json_to_proto_value(flow_json);
        let flow_struct = match flow_value.kind {
            Some(prost_wkt_types::value::Kind::StructValue(s)) => s,
            _ => {
                return Err(ClientError::InvalidResponse(
                    "Flow JSON must be an object".to_string(),
                ));
            }
        };

        let request = StoreFlowRequest {
            flow: Some(flow_struct),
            dry_run: false,
        };
        let response = self.flows.store_flow(request).await?.into_inner();
        Ok(response.flow_id)
    }

    /// Execute a flow synchronously, blocking until it completes, and return the output.
    ///
    /// This is equivalent to `submit` + waiting for the run to complete.
    pub async fn run(
        &mut self,
        flow_id: &str,
        input: serde_json::Value,
    ) -> ClientResult<serde_json::Value> {
        let input_proto = json_to_proto_value(input);

        let request = CreateRunRequest {
            flow_id: flow_id.to_string(),
            input: vec![input_proto],
            wait: true,
            ..Default::default()
        };
        let response = self.runs.create_run(request).await?.into_inner();

        // Extract first item's output from the run results
        if let Some(item) = response.results.first() {
            if let Some(output) = &item.output {
                return Ok(proto_value_to_json(output));
            }
            if let Some(msg) = &item.error_message {
                return Err(ClientError::InvalidResponse(format!("Run failed: {msg}")));
            }
        }

        Err(ClientError::InvalidResponse(
            "Run completed but returned no output".to_string(),
        ))
    }

    /// Submit a flow for asynchronous execution, returning the run ID.
    ///
    /// Use [`get_run`](Self::get_run) to poll for completion and
    /// [`get_run_items`](Self::get_run_items) to fetch outputs.
    pub async fn submit(
        &mut self,
        flow_id: &str,
        input: serde_json::Value,
    ) -> ClientResult<String> {
        let input_proto = json_to_proto_value(input);

        let request = CreateRunRequest {
            flow_id: flow_id.to_string(),
            input: vec![input_proto],
            wait: false,
            ..Default::default()
        };
        let response = self.runs.create_run(request).await?.into_inner();
        Ok(response.summary.map(|s| s.run_id).unwrap_or_default())
    }

    /// Get the status of a run.
    ///
    /// If `wait` is true, the request will block until the run completes (or fails).
    ///
    /// Note: outputs are not included in the response — use
    /// [`get_run_items`](Self::get_run_items) to fetch them after the run completes.
    pub async fn get_run(&mut self, run_id: &str, wait: bool) -> ClientResult<RunStatus> {
        let request = GetRunRequest {
            run_id: run_id.to_string(),
            wait,
            timeout_secs: None,
        };

        let response = self.runs.get_run(request).await?.into_inner();
        let summary = response.summary.unwrap_or_default();

        Ok(RunStatus {
            run_id: summary.run_id,
            status: summary.status,
            outputs: vec![],
        })
    }

    /// Get the output of each item in a completed run.
    ///
    /// Returns one `serde_json::Value` per input item, in submission order.
    /// Errors for individual items are currently surfaced as
    /// [`ClientError::InvalidResponse`].
    pub async fn get_run_items(&mut self, run_id: &str) -> ClientResult<Vec<serde_json::Value>> {
        let request = GetRunItemsRequest {
            run_id: run_id.to_string(),
            result_order: 0, // RESULT_ORDER_UNSPECIFIED
        };
        let response = self.runs.get_run_items(request).await?.into_inner();

        let mut outputs = Vec::with_capacity(response.results.len());
        for item in &response.results {
            if let Some(output) = &item.output {
                outputs.push(proto_value_to_json(output));
            } else if let Some(msg) = &item.error_message {
                return Err(ClientError::InvalidResponse(format!(
                    "Run item failed: {msg}"
                )));
            } else {
                outputs.push(serde_json::Value::Null);
            }
        }
        Ok(outputs)
    }

    /// List all components registered across all plugins.
    ///
    /// Set `exclude_schemas` to `true` to omit JSON Schemas from the response
    /// (faster when you only need component paths and descriptions).
    ///
    /// Note: this triggers on-demand component discovery from all plugins and
    /// may take a moment if workers haven't connected yet.
    pub async fn list_components(
        &mut self,
        exclude_schemas: bool,
    ) -> ClientResult<ListComponentsResult> {
        let request = ListRegisteredComponentsRequest { exclude_schemas };
        let response = self
            .components
            .list_registered_components(request)
            .await?
            .into_inner();

        let components = response
            .components
            .into_iter()
            .map(|c| ComponentInfo {
                component: c.component,
                description: c.description,
                input_schema: c.input_schema.map(proto_struct_to_json),
                output_schema: c.output_schema.map(proto_struct_to_json),
            })
            .collect();

        let failed_plugins = response
            .failed_plugins
            .into_iter()
            .map(|e| (e.plugin, e.error))
            .collect();

        Ok(ListComponentsResult {
            components,
            complete: response.complete,
            failed_plugins,
        })
    }

    /// Stream execution events for a run.
    ///
    /// Returns a server-streaming response that emits [`stepflow_proto::StatusEvent`]s as the
    /// run progresses.  Drive the stream with `stream.message().await`.
    ///
    /// Set `include_sub_runs` to also receive events from nested sub-flows.
    /// Set `include_results` to include step outputs in completion events.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example(mut client: stepflow_client::StepflowClient, run_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut stream = client.status_events(run_id, false, false).await?;
    /// while let Some(event) = stream.message().await? {
    ///     println!("{event:?}");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn status_events(
        &mut self,
        run_id: &str,
        include_sub_runs: bool,
        include_results: bool,
    ) -> ClientResult<StatusEventStream> {
        let request = GetRunEventsRequest {
            run_id: run_id.to_string(),
            since: None,
            event_types: vec![],
            include_sub_runs,
            include_results,
        };
        let stream = self.runs.get_run_events(request).await?.into_inner();
        Ok(stream)
    }

    /// Get the variable definitions declared in a flow.
    ///
    /// Returns a map of variable name → [`FlowVariable`] describing the schema,
    /// default value, and optional environment variable mapping for each variable.
    pub async fn get_flow_variables(
        &mut self,
        flow_id: &str,
    ) -> ClientResult<HashMap<String, FlowVariable>> {
        use stepflow_proto::GetFlowVariablesRequest;

        let request = GetFlowVariablesRequest {
            flow_id: flow_id.to_string(),
        };
        let response = self.flows.get_flow_variables(request).await?.into_inner();

        let variables = response
            .variables
            .into_iter()
            .map(|(name, v)| {
                (
                    name,
                    FlowVariable {
                        description: v.description,
                        default_value: v.default_value.as_ref().map(proto_value_to_json),
                        required: v.required,
                        schema: v.schema.map(proto_struct_to_json),
                        env_var: v.env_var,
                    },
                )
            })
            .collect();

        Ok(variables)
    }

    /// Check whether the orchestrator is healthy.
    pub async fn is_healthy(&mut self) -> bool {
        self.health
            .health_check(HealthCheckRequest {})
            .await
            .is_ok()
    }
}

// ---------------------------------------------------------------------------
// Proto ↔ JSON conversion helpers
// ---------------------------------------------------------------------------

/// Convert a `serde_json::Value` to `prost_wkt_types::Value`.
pub(crate) fn json_to_proto_value(value: serde_json::Value) -> prost_wkt_types::Value {
    use prost_wkt_types::value::Kind;
    prost_wkt_types::Value {
        kind: Some(match value {
            serde_json::Value::Null => Kind::NullValue(0),
            serde_json::Value::Bool(b) => Kind::BoolValue(b),
            serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => Kind::StringValue(s),
            serde_json::Value::Array(arr) => Kind::ListValue(prost_wkt_types::ListValue {
                values: arr.into_iter().map(json_to_proto_value).collect(),
            }),
            serde_json::Value::Object(obj) => Kind::StructValue(prost_wkt_types::Struct {
                fields: obj
                    .into_iter()
                    .map(|(k, v)| (k, json_to_proto_value(v)))
                    .collect(),
            }),
        }),
    }
}

/// Convert a `prost_wkt_types::Value` to `serde_json::Value`, preserving integer
/// types for whole-number floats (protobuf always uses f64 for numbers).
pub(crate) fn proto_value_to_json(value: &prost_wkt_types::Value) -> serde_json::Value {
    use prost_wkt_types::value::Kind;
    match &value.kind {
        Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::NumberValue(n)) => {
            let n = *n;
            if n.is_finite() && n.fract() == 0.0 {
                let i = n as i64;
                if i as f64 == n {
                    return serde_json::Value::Number(i.into());
                }
            }
            serde_json::Number::from_f64(n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::StructValue(s)) => {
            let map = s
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), proto_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(proto_value_to_json).collect())
        }
    }
}

/// Convert a `prost_wkt_types::Struct` to a `serde_json::Value::Object`.
fn proto_struct_to_json(s: prost_wkt_types::Struct) -> serde_json::Value {
    let map = s
        .fields
        .into_iter()
        .map(|(k, v)| (k, proto_value_to_json(&v)))
        .collect();
    serde_json::Value::Object(map)
}
