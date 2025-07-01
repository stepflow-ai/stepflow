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

use std::{collections::HashMap, sync::Arc};

use error_stack::ResultExt as _;
use stepflow_plugin::Context;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    sync::{
        mpsc::{self, error::TryRecvError},
        oneshot,
    },
};
use tokio_stream::{StreamExt as _, wrappers::LinesStream};

use crate::OwnedJson;
use crate::stdio::{Result, StdioError};
use crate::{Message, MessageHandlerRegistry, RequestId};

use super::launcher::Launcher;

struct ReceiveMessageLoop {
    child: Child,
    to_child: ChildStdin,
    from_child_stdout: LinesStream<BufReader<ChildStdout>>,
    from_child_stderr: LinesStream<BufReader<ChildStderr>>,
    pending_requests: HashMap<RequestId, oneshot::Sender<OwnedJson>>,
    outgoing_tx: mpsc::Sender<String>,
}

impl ReceiveMessageLoop {
    fn try_new(launcher: Launcher, outgoing_tx: mpsc::Sender<String>) -> Result<Self> {
        let mut child = launcher.spawn()?;

        let to_child = child.stdin.take().expect("stdin requested");
        let from_child_stdout = child.stdout.take().expect("stdout requested");
        let from_child_stdout = LinesStream::new(BufReader::new(from_child_stdout).lines());

        let from_child_stderr = child.stderr.take().expect("stderr requested");
        let from_child_stderr = LinesStream::new(BufReader::new(from_child_stderr).lines());

        Ok(Self {
            child,
            to_child,
            from_child_stdout,
            from_child_stderr,
            pending_requests: HashMap::new(),
            outgoing_tx,
        })
    }

    fn check_child_status(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait().change_context(StdioError::Spawn)? {
            if !status.success() {
                tracing::error!("Child process exited with status {status}");
                return Err(StdioError::Spawn.into());
            }
        }
        Ok(())
    }

    async fn send(&mut self, json: String) -> Result<()> {
        tracing::debug!("Sending message to child: {json}");
        self.to_child
            .write_all(json.as_bytes())
            .await
            .change_context(StdioError::Send)?;
        self.to_child
            .write_all(b"\n")
            .await
            .change_context(StdioError::Send)?;
        Ok(())
    }

    async fn iteration(
        &mut self,
        outgoing_rx: &mut mpsc::Receiver<String>,
        pending_rx: &mut mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
        context: &Arc<dyn Context>,
    ) -> Result<bool> {
        tokio::select! {
            child = self.child.wait() => {
                match child {
                    Ok(status) if status.success() => {
                        tracing::info!("Child process exited with status {status}");
                    }
                    Ok(status) => {
                        tracing::error!("Child process exited with status {status}");
                    }
                    Err(e) => {
                        tracing::error!("Child process exited with error: {e}");
                    }
                }
                Ok(false)
            }
            Some(outgoing) = outgoing_rx.recv() => {
                self.send(outgoing).await?;
                Ok(true)
            }
            Some(stderr_line) = self.from_child_stderr.next() => {
                let stderr_line = stderr_line.change_context(StdioError::Recv)?;
                tracing::info!("Component stderr: {stderr_line}");
                Ok(true)
            }
            Some(line) = self.from_child_stdout.next() => {
                let line = line.change_context(StdioError::Recv)?;
                tracing::info!("Received line from child: {line:?}");
                let msg = OwnedJson::try_new(line).change_context(StdioError::Recv)?;

                let message = msg.message();
                match message {
                    Message::Request(request) => {
                        tracing::info!("Received request for method '{}'", request.method);

                        let Some(handler) = MessageHandlerRegistry::instance().get_method_handler(request.method) else {
                            tracing::warn!("No handler found for method '{}'", request.method);

                            // Send an error response.
                            let response =  Message::Response(crate::MethodResponse::error(
                                request.id.clone(),
                                crate::Error::method_not_found(request.method),
                            ));
                            let response = serde_json::to_string(&response).change_context(StdioError::Send)?;
                            self.send(response).await?;
                            return Ok(true);
                        };

                        let outgoing_tx = self.outgoing_tx.clone();
                        let context = context.clone();
                        let future = async move {
                            let Message::Request(request) = msg.message() else {
                                panic!("Expected a request message");
                            };

                            if let Err(err) = handler.handle_message(request, outgoing_tx, context.clone()).await {
                                tracing::error!("Error handling request for method '{}': {:?}", request.method, err);
                            }
                        };
                        tokio::spawn(future);
                        Ok(true)
                    }
                    Message::Notification(notification) => {
                        tracing::error!("Received unsupported notification for method '{}'", notification.method);
                        Ok(true)
                    }
                    Message::Response(response) => {
                        tracing::info!("Received response with id '{}'", response.id);
                        if let Some(pending) = self.get_pending(pending_rx, &response.id) {
                            // Send the response to the pending request.
                            tracing::info!("Sending response to pending request with id '{}'", response.id);
                            pending.send(msg).map_err(|_| StdioError::Send)?;
                        }
                        Ok(true)
                    }
                }
            }
            else => {
                tracing::info!("Exiting recv loop");
                Ok(false)
            }
        }
    }

    /// Return the pending channel for the given request ID.
    fn get_pending(
        &mut self,
        pending_rx: &mut mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
        id: &RequestId,
    ) -> Option<oneshot::Sender<OwnedJson>> {
        if let Some(pending) = self.pending_requests.remove(id) {
            Some(pending)
        } else {
            // We haven't seen the pending request, so we'll receive from
            // the pending_rx channel until we find it.
            //
            // This shouldn't block the main loop much, since we
            // should have published to the pending channel
            // before sending the request -- if we've already
            // the response we believe it should be there.
            loop {
                match pending_rx.try_recv() {
                    Ok((pending_id, pending_request)) => {
                        if &pending_id == id {
                            return Some(pending_request);
                        }
                        self.pending_requests.insert(pending_id, pending_request);
                    }
                    Err(TryRecvError::Empty) => {
                        // No more pending requests. This means the response we got
                        // is unexpected. We'll log it and move on.
                        tracing::warn!("Unexpected response {id:?}");
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        // The pending_rx channel is closed, so we'll exit.
                        tracing::warn!("Pending channel is closed.");
                        break;
                    }
                }
            }
            None
        }
    }
}

pub async fn recv_message_loop(
    launcher: Launcher,
    outgoing_tx: mpsc::Sender<String>,
    mut outgoing_rx: mpsc::Receiver<String>,
    mut pending_rx: mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
    context: Arc<dyn Context>,
) -> Result<()> {
    let mut recv_loop = ReceiveMessageLoop::try_new(launcher, outgoing_tx)?;

    loop {
        match recv_loop
            .iteration(&mut outgoing_rx, &mut pending_rx, &context)
            .await
        {
            Ok(true) => {
                // Continue the loop.
            }
            Ok(false) => {
                // Exit the loop.
                break;
            }
            Err(mut e) => {
                tracing::info!("Error in recv loop: {e:?}. Checking child status.");
                if let Err(child_error) = recv_loop.check_child_status() {
                    e.extend_one(child_error);
                }
                return Err(StdioError::RecvLoop.into());
            }
        }
    }

    Ok(())
}
