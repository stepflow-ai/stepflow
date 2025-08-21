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

use std::sync::Arc;

use error_stack::ResultExt as _;
use serde::de::DeserializeOwned;
use stepflow_plugin::Context;

use crate::OwnedJson;
use crate::lazy_value::LazyValue;
use crate::protocol::{Method, ProtocolMethod, ProtocolNotification};
use crate::{MethodRequest, Notification, RequestId};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::Instrument as _;

use super::{launcher::Launcher, recv_message_loop::recv_message_loop};
use crate::error::{Result, TransportError};

/// Manages a client process spawned from a command.
///
/// Messages may be sent (as lines) to a channel that are sent via stdio.
pub struct StdioClient {
    outgoing_tx: mpsc::Sender<String>,
    pending_tx: mpsc::Sender<(RequestId, oneshot::Sender<OwnedJson>)>,
    // TODO: Use the handle. We should actually check it for errors
    // before bubbling other (less meaningful) errors up.
    #[allow(dead_code)]
    loop_handle: JoinHandle<Result<()>>,
}

impl StdioClient {
    pub async fn try_new(launcher: Launcher, context: Arc<dyn Context>) -> Result<Self> {
        let (outgoing_tx, outgoing_rx) = mpsc::channel(100);
        let (pending_tx, pending_rx) = mpsc::channel(100);

        let recv_span = tracing::info_span!("recv_message_loop", command = ?launcher.command, args = ?launcher.args);
        let loop_handle = tokio::spawn(
            recv_message_loop(
                launcher,
                outgoing_tx.clone(),
                outgoing_rx,
                pending_rx,
                context,
            )
            .instrument(recv_span),
        );

        Ok(Self {
            outgoing_tx,
            pending_tx,
            loop_handle,
        })
    }

    pub fn handle(&self) -> StdioClientHandle {
        StdioClientHandle {
            outgoing_tx: self.outgoing_tx.clone(),
            pending_tx: self.pending_tx.clone(),
        }
    }
}

#[derive(Clone)]
pub struct StdioClientHandle {
    outgoing_tx: mpsc::Sender<String>,
    /// Channel to send new pending requests to.
    pending_tx: mpsc::Sender<(RequestId, oneshot::Sender<OwnedJson>)>,
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

        self.outgoing_tx
            .send(msg)
            .await
            .change_context(TransportError::Send)?;

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
            .map_err(|_| TransportError::Send)?;

        let request = MethodRequest::new(id.clone(), method, Some(params));
        self.send(&request).await?;

        let response = response_rx
            .await
            .change_context(TransportError::Recv)?
            .owned_response()?;

        // This is an assertion since the routing should only send the response for the
        // registered ID to the pending one-shot channel.
        debug_assert_eq!(response.response().id(), &id);
        response.into_success_value()
    }
}
