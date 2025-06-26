use std::sync::Arc;

use error_stack::ResultExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result,
};
use tokio::sync::RwLock;

use super::{
    StdioError,
    client::{Client, ClientHandle},
    launcher::Launcher,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct StdioPluginConfig {
    pub command: String,
    pub args: Vec<String>,
    /// Environment variables to pass to the sub-process.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
}

impl PluginConfig for StdioPluginConfig {
    type Error = StdioError;

    async fn create_plugin(
        self,
        working_directory: &std::path::Path,
        protocol_prefix: &str,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        // Look for the command in the system path and the working directory.
        let Self { command, args, env } = self;

        let launcher = Launcher::try_new(working_directory.to_owned(), command, args, env)?;

        Ok(DynPlugin::boxed(StdioPlugin::new(
            launcher,
            protocol_prefix.to_string(),
        )))
    }
}

pub struct StdioPlugin {
    state: RwLock<StdioPluginState>,
    protocol_prefix: String,
}

impl StdioPlugin {
    pub fn new(launcher: Launcher, protocol_prefix: String) -> Self {
        Self {
            state: RwLock::new(StdioPluginState::Uninitialized(launcher)),
            protocol_prefix,
        }
    }
}

enum StdioPluginState {
    Empty,
    Uninitialized(Launcher),
    Initialized(ClientHandle),
}

impl StdioPlugin {
    async fn client_handle(&self) -> Result<ClientHandle> {
        let guard = self.state.read().await;
        match &*guard {
            StdioPluginState::Initialized(handle) => Ok(handle.clone()),
            _ => Err(PluginError::Execution).attach_printable("stdio client not initialized"),
        }
    }

    async fn create_client(&self, context: Arc<dyn Context>) -> Result<ClientHandle> {
        let mut guard = self.state.write().await;
        match std::mem::replace(&mut *guard, StdioPluginState::Empty) {
            StdioPluginState::Uninitialized(launcher) => {
                let client = Client::try_new(launcher, context)
                    .await
                    .change_context(PluginError::Initializing)
                    .attach_printable("Unable to launch component server")?;
                *guard = StdioPluginState::Initialized(client.handle());
                Ok(client.handle())
            }
            _ => Err(PluginError::Initializing)
                .attach_printable_lazy(|| "Unexpected state".to_string()),
        }
    }
}

impl Plugin for StdioPlugin {
    async fn init(&self, context: &Arc<dyn Context>) -> Result<()> {
        let client = self.create_client(context.clone()).await?;

        client
            .request(&crate::schema::initialization::Request {
                runtime_protocol_version: 1,
                protocol_prefix: self.protocol_prefix.clone(),
            })
            .await
            .change_context(PluginError::Initializing)?;

        client
            .notify(&crate::schema::initialization::Complete {})
            .await
            .change_context(PluginError::Initializing)?;

        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<Component>> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .request(&crate::schema::list_components::Request {})
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.components)
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        // TODO: Enrich this? Component not found, etc. based on the protocol error code?
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .request(&crate::schema::component_info::Request {
                component: component.clone(),
            })
            .await
            .change_context(PluginError::ComponentInfo)?;

        Ok(response.info)
    }

    async fn execute(
        &self,
        component: &Component,
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let client_handle = self.client_handle().await?;
        let response = client_handle
            .request(&crate::schema::component_execute::Request {
                component: component.clone(),
                input,
                execution_id: context.execution_id(),
                step_id: context.step_id().unwrap_or("unknown").to_string(),
            })
            .await
            .change_context(PluginError::Execution)?;

        tracing::debug!("StdioPlugin: Received response: {:?}", response.output);

        // Check if the response contains a FlowResult by looking for the "outcome" field
        if let Some(outcome_obj) = response.output.as_object() {
            if let Some(outcome_value) = outcome_obj.get("outcome") {
                if let Some(outcome_str) = outcome_value.as_str() {
                    tracing::debug!("StdioPlugin: Found outcome field with value: {}", outcome_str);
                    // If the response has an "outcome" field, try to deserialize it as a FlowResult
                    match outcome_str {
                        "streaming" => {
                            // Try to deserialize as FlowResult::Streaming
                            tracing::debug!("StdioPlugin: Attempting to deserialize streaming result");
                            match serde_json::from_value::<stepflow_core::FlowResult>(response.output.as_ref().clone()) {
                                Ok(flow_result) => {
                                    tracing::info!("StdioPlugin: Successfully deserialized streaming result");
                                    return Ok(flow_result);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to deserialize streaming result: {}, attempting flexible construction", e);
                                    // Try to flexibly construct FlowResult::Streaming by adapting the structure
                                    if let Some(obj) = response.output.as_ref().as_object() {
                                        // Try to extract core streaming fields with flexible typing
                                        let stream_id = obj.get("stream_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown");

                                        let chunk = obj.get("chunk")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");

                                        let chunk_index = obj.get("chunk_index")
                                            .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|i| i as u64)))
                                            .unwrap_or(0) as usize;

                                        let is_final = obj.get("is_final")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);

                                        // If we have a metadata field, use it; otherwise create one from remaining fields
                                        let metadata = if let Some(existing_metadata) = obj.get("metadata") {
                                            stepflow_core::workflow::ValueRef::new(existing_metadata.clone())
                                        } else {
                                            // Create metadata from all fields except the core streaming ones
                                            let mut metadata_obj = serde_json::Map::new();
                                            for (key, value) in obj.iter() {
                                                if !matches!(key.as_str(), "outcome" | "stream_id" | "chunk" | "chunk_index" | "is_final") {
                                                    metadata_obj.insert(key.clone(), value.clone());
                                                }
                                            }
                                            stepflow_core::workflow::ValueRef::new(serde_json::Value::Object(metadata_obj))
                                        };

                                        let flow_result = stepflow_core::FlowResult::Streaming {
                                            stream_id: stream_id.to_string(),
                                            metadata,
                                            chunk: chunk.to_string(),
                                            chunk_index,
                                            is_final,
                                        };

                                        tracing::info!("StdioPlugin: Successfully constructed streaming result flexibly");
                                        return Ok(flow_result);
                                    }
                                    tracing::warn!("Failed to flexibly construct streaming result, falling back to Success");
                                }
                            }
                        }
                        "success" | "failed" | "skipped" => {
                            // Try to deserialize as any FlowResult variant
                            match serde_json::from_value::<stepflow_core::FlowResult>(response.output.as_ref().clone()) {
                                Ok(flow_result) => {
                                    tracing::debug!("StdioPlugin: Successfully deserialized FlowResult: {:?}", flow_result);
                                    return Ok(flow_result);
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to deserialize FlowResult: {}, falling back to Success", e);
                                }
                            }
                        }
                        _ => {
                            // Unknown outcome, treat as regular success
                            tracing::debug!("StdioPlugin: Unknown outcome '{}', treating as regular success", outcome_str);
                        }
                    }
                } else {
                    tracing::debug!("StdioPlugin: outcome field is not a string");
                }
            } else {
                tracing::debug!("StdioPlugin: No outcome field found in response");
            }
        } else {
            tracing::debug!("StdioPlugin: Response output is not an object");
        }

        // Default behavior: wrap in Success
        Ok(FlowResult::Success {
            result: response.output,
        })
    }
}
