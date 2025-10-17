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

use std::ops::DerefMut as _;
use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::de::DeserializeOwned;
use stepflow_plugin::Context;

use super::{launcher::Launcher, recv_message_loop::recv_message_loop};
use crate::OwnedJson;
use crate::error::{Result, TransportError};
use crate::lazy_value::LazyValue;
use crate::protocol::{Method, ProtocolMethod, ProtocolNotification};
use crate::{MethodRequest, Notification, RequestId};
use tokio::{
    sync::RwLock,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

/// Restart counter that increments each time the subprocess is restarted
pub type RestartCounter = u64;

/// Manages a client process spawned from a command.
///
/// Messages may be sent (as lines) to a channel that are sent via stdio.
pub struct StdioClient {
    outgoing_tx: mpsc::Sender<String>,
    pending_tx: mpsc::Sender<(RequestId, oneshot::Sender<OwnedJson>)>,
    loop_handle: Arc<RwLock<JoinHandle<Result<()>>>>,
    restart_counter_tx: watch::Sender<RestartCounter>,
}

impl StdioClient {
    pub async fn try_new(launcher: Launcher, context: Arc<dyn Context>) -> Result<Self> {
        let (outgoing_tx, outgoing_rx) = mpsc::channel(100);
        let (pending_tx, pending_rx) = mpsc::channel(100);
        let (restart_counter_tx, _restart_counter_rx) = watch::channel(0);

        let launcher = Arc::new(launcher);
        log::info!(
            "Starting recv_message_loop for command: {:?} with args: {:?}",
            launcher.command,
            launcher.args
        );
        let loop_handle = tokio::spawn(recv_message_loop(
            launcher,
            outgoing_tx.clone(),
            outgoing_rx,
            pending_rx,
            context,
            restart_counter_tx.clone(),
        ));
        let loop_handle = Arc::new(RwLock::new(loop_handle));

        Ok(Self {
            outgoing_tx,
            pending_tx,
            loop_handle,
            restart_counter_tx,
        })
    }

    pub fn handle(&self) -> StdioClientHandle {
        StdioClientHandle {
            outgoing_tx: self.outgoing_tx.clone(),
            pending_tx: self.pending_tx.clone(),
            loop_handle: self.loop_handle.clone(),
            restart_counter_tx: self.restart_counter_tx.clone(),
        }
    }
}

#[derive(Clone)]
pub struct StdioClientHandle {
    /// Channel to send outgoing JSON lines to.
    outgoing_tx: mpsc::Sender<String>,
    /// Channel to send new pending requests to.
    pending_tx: mpsc::Sender<(RequestId, oneshot::Sender<OwnedJson>)>,
    loop_handle: Arc<RwLock<JoinHandle<Result<()>>>>,
    restart_counter_tx: watch::Sender<RestartCounter>,
}

impl StdioClientHandle {
    pub async fn method<I>(&self, params: &I) -> Result<I::Response>
    where
        I: ProtocolMethod + serde::Serialize + Send + Sync + std::fmt::Debug,
        I::Response: DeserializeOwned + Send + Sync + 'static,
    {
        let response = self
            .method_dyn(I::METHOD_NAME, LazyValue::write_ref(params))
            .await?;
        response
            .value()
            .deserialize_to::<I::Response>()
            .change_context(TransportError::InvalidResponse(I::METHOD_NAME))
    }

    pub async fn notify<I>(&self, params: &I) -> Result<()>
    where
        I: ProtocolNotification + serde::Serialize + Send + Sync + std::fmt::Debug,
    {
        self.send(&Notification::new(
            I::METHOD_NAME,
            Some(LazyValue::write_ref(params)),
        ))
        .await?;

        Ok(())
    }

    async fn send(&self, msg: &(dyn erased_serde::Serialize + Send + Sync)) -> Result<()> {
        let msg = serde_json::to_string(&msg).change_context(TransportError::Send)?;

        let send_result = self.outgoing_tx.send(msg).await;
        self.handle_channel_error(send_result, TransportError::Send)
            .await?;

        Ok(())
    }

    async fn method_dyn(
        &self,
        method: Method,
        params: LazyValue<'_>,
    ) -> Result<OwnedJson<LazyValue<'static>>> {
        let id = RequestId::new_uuid();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.pending_tx
            .send((id.clone(), response_tx))
            .await
            .change_context(TransportError::Send)?;

        let request = MethodRequest::new(id.clone(), method, Some(params));
        self.send(&request).await?;
        let response = response_rx.await;
        let response = self
            .handle_channel_error(response, TransportError::Recv)
            .await?
            .owned_response()?;

        // This is an assertion since the routing should only send the response for the
        // registered ID to the pending one-shot channel.
        debug_assert_eq!(response.response().id(), &id);
        response.into_success_value()
    }

    /// Get current restart counter and a receiver to watch for restart count increases
    /// Returns the current restart count and a receiver to monitor future increments
    pub fn restart_counter(&self) -> (RestartCounter, watch::Receiver<RestartCounter>) {
        let current_count = *self.restart_counter_tx.borrow();
        let receiver = self.restart_counter_tx.subscribe();
        (current_count, receiver)
    }

    async fn handle_channel_error<T, E: error_stack::Context>(
        &self,
        result: Result<T, E>,
        transport_error: TransportError,
    ) -> Result<T> {
        match result {
            Ok(value) => Ok(value),
            Err(e) => {
                if !self.loop_handle.read().await.is_finished() {
                    return Err(error_stack::report!(e).change_context(transport_error));
                }

                let mut write = self.loop_handle.write().await;
                let result = write.deref_mut().await;
                match result {
                    Ok(Ok(())) => {
                        log::error!("Subprocess exited successfully.");
                    }
                    Ok(Err(e)) => {
                        log::error!("Error running receive loop: {e})");
                    }
                    Err(e) => {
                        log::error!("Panic in receive loop: {e}");
                    }
                };
                error_stack::bail!(transport_error)
            }
        }
    }
}
