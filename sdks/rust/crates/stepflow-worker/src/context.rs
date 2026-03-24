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

//! Component execution context — blob storage and orchestrator callbacks.

use tonic::transport::Channel;

use crate::error::ContextError;

/// Run status returned by [`ComponentContext::get_run`] and [`ComponentContext::submit_run`].
#[derive(Debug, Clone)]
pub struct RunStatus {
    /// The run's unique identifier.
    pub run_id: String,
    /// Numeric execution status (see `ExecutionStatus` proto enum values).
    pub status: i32,
    /// Outputs for each item in the run (populated when `include_results` is true).
    pub outputs: Vec<serde_json::Value>,
}

/// Context provided to each component during execution.
///
/// Provides access to blob storage and bidirectional orchestrator communication.
/// The context is inexpensive to clone — it shares the underlying gRPC channels.
#[derive(Clone)]
pub struct ComponentContext {
    pub(crate) run_id: Option<String>,
    pub(crate) root_run_id: Option<String>,
    pub(crate) flow_id: Option<String>,
    pub(crate) step_id: Option<String>,
    pub(crate) attempt: u32,
    /// Channel to the OrchestratorService (for SubmitRun, GetRun, Heartbeat).
    orch_channel: Channel,
    /// Dedicated channel to the BlobService (may differ from orch_channel when
    /// `STEPFLOW_BLOB_URL` is set to avoid overloading the orchestrator).
    blob_channel: Channel,
}

impl ComponentContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        blob_url: Option<String>,
        run_id: Option<String>,
        root_run_id: Option<String>,
        flow_id: Option<String>,
        step_id: Option<String>,
        attempt: u32,
        orch_channel: Channel,
    ) -> Self {
        // Build a dedicated blob channel when a separate blob URL is configured.
        // Using connect_lazy() so the connection is established on first use (sync).
        let blob_channel = blob_url
            .as_deref()
            .and_then(|url| {
                tonic::transport::Endpoint::from_shared(url.to_string())
                    .ok()
                    .map(|e| e.connect_lazy())
            })
            .unwrap_or_else(|| orch_channel.clone());

        Self {
            run_id,
            root_run_id,
            flow_id,
            step_id,
            attempt,
            orch_channel,
            blob_channel,
        }
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// The current run ID.
    pub fn run_id(&self) -> Option<&str> {
        self.run_id.as_deref()
    }

    /// The root run ID (for nested flows, this is the top-level run).
    pub fn root_run_id(&self) -> Option<&str> {
        self.root_run_id.as_deref()
    }

    /// The flow ID of the flow being executed.
    pub fn flow_id(&self) -> Option<&str> {
        self.flow_id.as_deref()
    }

    /// The step ID being executed.
    pub fn step_id(&self) -> Option<&str> {
        self.step_id.as_deref()
    }

    /// The current attempt number (1-based; increments on retry).
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    // -----------------------------------------------------------------------
    // Blob storage
    // -----------------------------------------------------------------------

    /// Store a JSON value as a content-addressed blob.
    ///
    /// Returns the blob ID (SHA-256 hash) that can be used to retrieve it later.
    pub async fn put_blob(&self, data: serde_json::Value) -> Result<String, ContextError> {
        use stepflow_proto::{
            PutBlobRequest, blob_service_client::BlobServiceClient, put_blob_request::Content,
        };

        let value = json_to_proto_value(data);

        let mut client = BlobServiceClient::new(self.blob_channel.clone());
        let response = client
            .put_blob(PutBlobRequest {
                content: Some(Content::JsonData(value)),
                blob_type: "data".to_string(),
                filename: None,
                content_type: None,
            })
            .await?
            .into_inner();

        Ok(response.blob_id)
    }

    /// Retrieve a blob by its ID.
    pub async fn get_blob(&self, blob_id: &str) -> Result<serde_json::Value, ContextError> {
        use stepflow_proto::{
            GetBlobRequest, blob_service_client::BlobServiceClient, get_blob_response::Content,
        };

        let mut client = BlobServiceClient::new(self.blob_channel.clone());
        let response = client
            .get_blob(GetBlobRequest {
                blob_id: blob_id.to_string(),
            })
            .await?
            .into_inner();

        let content = response.content.ok_or_else(|| {
            ContextError::BlobFailed(format!("Blob {blob_id} returned no content"))
        })?;

        let value = match content {
            Content::JsonData(v) => v,
            Content::RawData(_) => {
                return Err(ContextError::BlobFailed(
                    "Expected JSON blob but got binary data".to_string(),
                ));
            }
        };

        Ok(proto_value_to_json(&value))
    }

    /// Store raw binary data as a content-addressed blob.
    ///
    /// Returns the blob ID (SHA-256 hash). Use [`get_blob_binary`](Self::get_blob_binary)
    /// to retrieve it.
    pub async fn put_blob_binary(&self, data: Vec<u8>) -> Result<String, ContextError> {
        use stepflow_proto::{
            PutBlobRequest, blob_service_client::BlobServiceClient, put_blob_request::Content,
        };

        let mut client = BlobServiceClient::new(self.blob_channel.clone());
        let response = client
            .put_blob(PutBlobRequest {
                content: Some(Content::RawData(data)),
                blob_type: "data".to_string(),
                filename: None,
                content_type: None,
            })
            .await?
            .into_inner();

        Ok(response.blob_id)
    }

    /// Retrieve a binary blob by its ID.
    pub async fn get_blob_binary(&self, blob_id: &str) -> Result<Vec<u8>, ContextError> {
        use stepflow_proto::{
            GetBlobRequest, blob_service_client::BlobServiceClient, get_blob_response::Content,
        };

        let mut client = BlobServiceClient::new(self.blob_channel.clone());
        let response = client
            .get_blob(GetBlobRequest {
                blob_id: blob_id.to_string(),
            })
            .await?
            .into_inner();

        let content = response.content.ok_or_else(|| {
            ContextError::BlobFailed(format!("Blob {blob_id} returned no content"))
        })?;

        match content {
            Content::RawData(bytes) => Ok(bytes),
            Content::JsonData(_) => Err(ContextError::BlobFailed(
                "Expected binary blob but got JSON data".to_string(),
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Run submission
    // -----------------------------------------------------------------------

    /// Submit a nested flow run.
    ///
    /// `flow_id` is the blob ID of the flow definition (store with [`put_blob`](Self::put_blob)
    /// using `blob_type = "flow"`, or use `store_flow` via the `stepflow-client` crate).
    ///
    /// `inputs` is a list of inputs — use a single-element list for single-input flows.
    ///
    /// If `wait` is true the call blocks until the run completes.
    pub async fn submit_run(
        &self,
        flow_id: &str,
        inputs: Vec<serde_json::Value>,
        wait: bool,
    ) -> Result<RunStatus, ContextError> {
        use stepflow_proto::{
            CreateRunRequest, OrchestratorSubmitRunRequest,
            orchestrator_service_client::OrchestratorServiceClient,
        };

        let proto_inputs: Vec<prost_wkt_types::Value> =
            inputs.into_iter().map(json_to_proto_value).collect();

        let mut client = OrchestratorServiceClient::new(self.orch_channel.clone());
        let response = client
            .submit_run(OrchestratorSubmitRunRequest {
                run_request: Some(CreateRunRequest {
                    flow_id: flow_id.to_string(),
                    input: proto_inputs,
                    wait,
                    ..Default::default()
                }),
                subflow_key: None,
                root_run_id: self.root_run_id.clone(),
            })
            .await?
            .into_inner();

        let outputs = response
            .results
            .iter()
            .filter_map(|item| item.output.as_ref())
            .map(proto_value_to_json)
            .collect();

        Ok(RunStatus {
            run_id: response.run_id,
            status: response.status,
            outputs,
        })
    }

    /// Get the status of a run.
    ///
    /// If `wait` is true the call blocks until the run completes.
    /// If `include_results` is true the response will include output values.
    pub async fn get_run(
        &self,
        run_id: &str,
        wait: bool,
        include_results: bool,
    ) -> Result<RunStatus, ContextError> {
        use stepflow_proto::{
            OrchestratorGetRunRequest, orchestrator_service_client::OrchestratorServiceClient,
        };

        let mut client = OrchestratorServiceClient::new(self.orch_channel.clone());
        let response = client
            .get_run(OrchestratorGetRunRequest {
                run_id: run_id.to_string(),
                wait,
                include_results,
                result_order: 0, // RESULT_ORDER_UNSPECIFIED
                timeout_secs: None,
                root_run_id: self.root_run_id.clone(),
            })
            .await?
            .into_inner();

        let outputs = response
            .results
            .iter()
            .filter_map(|item| item.output.as_ref())
            .map(proto_value_to_json)
            .collect();

        Ok(RunStatus {
            run_id: response.run_id,
            status: response.status,
            outputs,
        })
    }

    /// Store a flow definition as a blob and immediately execute it, returning the first output.
    ///
    /// The `flow_json` should be a valid serialized Stepflow flow (from
    /// `Flow::to_json()` in the `stepflow-client` crate).
    pub async fn evaluate_flow(
        &self,
        flow_json: serde_json::Value,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ContextError> {
        use stepflow_proto::{
            PutBlobRequest, blob_service_client::BlobServiceClient, put_blob_request::Content,
        };

        // Store the flow as a blob
        let flow_value = json_to_proto_value(flow_json);
        let mut blob_client = BlobServiceClient::new(self.blob_channel.clone());
        let store_response = blob_client
            .put_blob(PutBlobRequest {
                content: Some(Content::JsonData(flow_value)),
                blob_type: "flow".to_string(),
                filename: None,
                content_type: None,
            })
            .await?
            .into_inner();

        let flow_id = store_response.blob_id;

        // Execute it
        let status = self.submit_run(&flow_id, vec![input], true).await?;

        status
            .outputs
            .into_iter()
            .next()
            .ok_or_else(|| ContextError::RunFailed("Flow produced no output".to_string()))
    }
}

/// Convert a `prost_wkt_types::Value` to `serde_json::Value`, preserving integer
/// types for whole-number floats.
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
