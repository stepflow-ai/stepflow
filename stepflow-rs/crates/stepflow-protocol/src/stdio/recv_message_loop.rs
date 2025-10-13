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

use std::{collections::HashMap, sync::Arc, time::Duration};

use error_stack::ResultExt as _;
use stepflow_plugin::Context;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    sync::{
        mpsc::{self, error::TryRecvError},
        oneshot, watch,
    },
    time::sleep,
};
use tokio_stream::{StreamExt as _, wrappers::LinesStream};

use crate::OwnedJson;
use crate::error::{Result, TransportError};
use crate::{Message, MessageHandlerRegistry, RequestId};

use super::{client::RestartCounter, launcher::Launcher};

struct ReceiveMessageLoop {
    child: Child,
    to_child: ChildStdin,
    from_child_stdout: LinesStream<BufReader<ChildStdout>>,
    from_child_stderr: LinesStream<BufReader<ChildStderr>>,
    pending_requests: HashMap<RequestId, oneshot::Sender<OwnedJson>>,
    outgoing_tx: mpsc::Sender<String>,
    launcher: Arc<Launcher>,
}

impl ReceiveMessageLoop {
    fn try_new(launcher: Arc<Launcher>, outgoing_tx: mpsc::Sender<String>) -> Result<Self> {
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let mut child = launcher.spawn(&env)?;

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
            launcher,
        })
    }

    /// Clear in-flight requests that were sent to the old process (called on process restart)
    fn clear_inflight_requests(
        &mut self,
        pending_rx: &mut mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
    ) {
        let count = self.pending_requests.len();
        tracing::debug!("clear_inflight_requests called, found {count} in-flight requests in map");

        // Clear requests already in the HashMap - these were sent to the old process
        // and will never receive responses after restart
        if count > 0 {
            tracing::warn!("Clearing {count} in-flight requests from map due to process restart");
            for (request_id, sender) in self.pending_requests.drain() {
                tracing::debug!("Closing in-flight request from map: {request_id}");
                // Send a transport error response to close the waiting client
                let error_response = OwnedJson::try_new(
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request_id,
                        "error": {
                            "code": -32603,
                            "message": "Process crashed during request execution"
                        }
                    })
                    .to_string(),
                );

                match error_response {
                    Ok(response) => {
                        if sender.send(response).is_err() {
                            tracing::debug!(
                                "Failed to send error response to closed request {request_id}"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to create error response for request {request_id}: {e:?}"
                        );
                        // Drop the sender to signal an error
                    }
                }
            }
            tracing::info!("Cleared {count} in-flight requests that were sent to old process");
        } else {
            tracing::debug!("No in-flight requests to clear during restart");
        }

        // Also clear any pending requests still in the channel that were sent during the crash
        let mut pending_count = 0;
        loop {
            match pending_rx.try_recv() {
                Ok((request_id, sender)) => {
                    pending_count += 1;
                    tracing::debug!(
                        "Closing pending request from channel during restart: {request_id}"
                    );
                    // Send error response similar to in-flight requests
                    let error_response = OwnedJson::try_new(
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "error": {
                                "code": -32603,
                                "message": "Process crashed during request execution"
                            }
                        })
                        .to_string(),
                    );

                    match error_response {
                        Ok(response) => {
                            if sender.send(response).is_err() {
                                tracing::debug!(
                                    "Failed to send error response to closed pending request {request_id}"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to create error response for pending request {request_id}: {e:?}"
                            );
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    tracing::warn!("Pending channel is disconnected during restart");
                    break;
                }
            }
        }
        if pending_count > 0 {
            tracing::info!("Cleared {pending_count} pending requests from channel during restart");
        }
    }

    /// Drain remaining output from crashed process before restarting.
    ///
    /// When a process crashes, there may be stderr/stdout lines still buffered
    /// in the pipes (like exception tracebacks). This method attempts to read
    /// all remaining output with a timeout to capture crash diagnostics.
    async fn drain_remaining_output(&mut self) {
        use tokio::time::{timeout, Duration};

        tracing::info!("Draining remaining output from crashed process...");

        // Use a short timeout to avoid blocking restart too long
        let drain_timeout = Duration::from_millis(500);
        let mut lines_drained = 0;

        loop {
            match timeout(drain_timeout, async {
                tokio::select! {
                    Some(stderr_line) = self.from_child_stderr.next() => {
                        if let Ok(line) = stderr_line {
                            tracing::info!("Component stderr (on crash): {line}");
                            lines_drained += 1;
                        }
                        true
                    }
                    Some(stdout_line) = self.from_child_stdout.next() => {
                        if let Ok(line) = stdout_line {
                            tracing::info!("Component stdout (on crash): {line}");
                            lines_drained += 1;
                        }
                        true
                    }
                    else => false
                }
            })
            .await
            {
                Ok(true) => continue,      // Got a line, keep draining
                Ok(false) | Err(_) => break, // No more lines or timeout
            }
        }

        if lines_drained > 0 {
            tracing::info!(
                "Drained {lines_drained} lines from crashed process output"
            );
        } else {
            tracing::info!("No remaining output from crashed process");
        }
    }

    /// Restart the process with the same launcher configuration
    async fn restart_process(
        &mut self,
        pending_rx: &mut mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
    ) -> Result<()> {
        tracing::info!(
            "Restarting process: {:?} {:?}",
            self.launcher.command,
            self.launcher.args
        );

        // Drain any remaining output from the crashed process before restarting
        // This captures stderr/stdout that may contain crash diagnostics (tracebacks, errors)
        self.drain_remaining_output().await;

        // Clear in-flight requests that were sent to the old process
        // Also clear pending requests in the channel that were sent during the crash
        self.clear_inflight_requests(pending_rx);

        // Kill the old process if it's still running
        if let Err(e) = self.child.start_kill() {
            tracing::warn!("Failed to kill old process: {e}");
        }

        // Spawn a new process
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let mut new_child = self.launcher.spawn(&env)?;

        let new_stdin = new_child.stdin.take().expect("stdin requested");
        let new_stdout = new_child.stdout.take().expect("stdout requested");
        let new_stdout = LinesStream::new(BufReader::new(new_stdout).lines());

        let new_stderr = new_child.stderr.take().expect("stderr requested");
        let new_stderr = LinesStream::new(BufReader::new(new_stderr).lines());

        // Replace the old process and streams
        self.child = new_child;
        self.to_child = new_stdin;
        self.from_child_stdout = new_stdout;
        self.from_child_stderr = new_stderr;

        tracing::info!("Process restart completed successfully");
        Ok(())
    }

    fn check_child_status(&mut self) -> Result<()> {
        if let Some(status) = self
            .child
            .try_wait()
            .change_context(TransportError::Spawn)?
            && !status.success()
        {
            tracing::error!("Child process exited with status {status}");
            return Err(TransportError::Spawn.into());
        }
        Ok(())
    }

    async fn send(&mut self, json: String) -> Result<()> {
        tracing::debug!("Sending message to child: {json}");
        self.to_child
            .write_all(json.as_bytes())
            .await
            .change_context(TransportError::Send)?;
        self.to_child
            .write_all(b"\n")
            .await
            .change_context(TransportError::Send)?;
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
                let stderr_line = stderr_line.change_context(TransportError::Recv)?;
                tracing::info!("Component stderr: {stderr_line}");
                Ok(true)
            }
            Some(line) = self.from_child_stdout.next() => {
                let line = line.change_context(TransportError::Recv)?;
                let msg = OwnedJson::try_new(line).change_context(TransportError::Recv)?;

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
                            let response = serde_json::to_string(&response).change_context(TransportError::Send)?;
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
                        tracing::info!("Received response with id '{}'", response.id());
                        if let Some(pending) = self.get_pending(pending_rx, response.id()) {
                            // Send the response to the pending request.
                            tracing::info!("Sending response to pending request with id '{}'", response.id());
                            pending.send(msg).map_err(|_| TransportError::Send)?;
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
            tracing::debug!("Found pending request {id} in map");
            Some(pending)
        } else {
            // We haven't seen the pending request, so we'll receive from
            // the pending_rx channel until we find it.
            //
            // This shouldn't block the main loop much, since we
            // should have published to the pending channel
            // before sending the request -- if we've already
            // the response we believe it should be there.
            tracing::debug!("Pending request {id} not in map, checking pending_rx channel");
            loop {
                match pending_rx.try_recv() {
                    Ok((pending_id, pending_request)) => {
                        tracing::debug!("Received pending request {pending_id} from channel");
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
    launcher: Arc<Launcher>,
    outgoing_tx: mpsc::Sender<String>,
    mut outgoing_rx: mpsc::Receiver<String>,
    mut pending_rx: mpsc::Receiver<(RequestId, oneshot::Sender<OwnedJson>)>,
    context: Arc<dyn Context>,
    restart_counter_tx: watch::Sender<RestartCounter>,
) -> Result<()> {
    let mut recv_loop = ReceiveMessageLoop::try_new(launcher, outgoing_tx)?;
    let mut restart_count = 0;
    let max_restart_attempts = 5;
    let mut backoff_duration = Duration::from_secs(1);
    const MAX_BACKOFF: Duration = Duration::from_secs(30);

    loop {
        match recv_loop
            .iteration(&mut outgoing_rx, &mut pending_rx, &context)
            .await
        {
            Ok(true) => {
                // Continue the loop - reset restart count on successful operation
                restart_count = 0;
                backoff_duration = Duration::from_secs(1);
            }
            Ok(false) => {
                // Process exited cleanly - check if it was successful or not
                if let Some(status) = recv_loop
                    .child
                    .try_wait()
                    .change_context(TransportError::Spawn)?
                {
                    if status.success() {
                        tracing::info!("Process exited successfully, stopping recv loop");
                        break;
                    } else {
                        tracing::warn!("Process exited with failure status: {status}");
                        // Treat failed exit as restart case
                    }
                } else {
                    tracing::info!("Process termination detected, stopping recv loop");
                    break;
                }

                // Attempt restart for failed exits
                if restart_count >= max_restart_attempts {
                    tracing::error!(
                        "Maximum restart attempts ({max_restart_attempts}) exceeded, giving up"
                    );
                    return Err(TransportError::RecvLoop.into());
                }

                restart_count += 1;
                tracing::warn!(
                    "Attempting restart {restart_count}/{max_restart_attempts} after {backoff_duration:?} delay"
                );

                sleep(backoff_duration).await;

                match recv_loop.restart_process(&mut pending_rx).await {
                    Ok(()) => {
                        tracing::info!(
                            "Process restart {restart_count}/{max_restart_attempts} successful"
                        );
                        // Double the backoff for next time, up to maximum
                        backoff_duration = std::cmp::min(backoff_duration * 2, MAX_BACKOFF);

                        // Notify restart completion
                        if let Err(e) = restart_counter_tx.send(restart_count) {
                            tracing::warn!("Failed to send restart counter: {e:?}");
                        }
                        continue;
                    }
                    Err(restart_error) => {
                        tracing::error!(
                            "Process restart {restart_count}/{max_restart_attempts} failed: {restart_error:?}"
                        );
                        backoff_duration = std::cmp::min(backoff_duration * 2, MAX_BACKOFF);
                        continue; // Try again on next iteration
                    }
                }
            }
            Err(mut e) => {
                tracing::warn!("Error in recv loop: {e:?}. Checking child status.");
                if let Err(child_error) = recv_loop.check_child_status() {
                    e.extend_one(child_error);
                }

                // Attempt restart for errors too
                if restart_count >= max_restart_attempts {
                    tracing::error!(
                        "Maximum restart attempts ({max_restart_attempts}) exceeded after error, giving up: {e:?}"
                    );
                    return Err(TransportError::RecvLoop.into());
                }

                restart_count += 1;
                tracing::warn!(
                    "Attempting restart {restart_count}/{max_restart_attempts} after error, delay: {backoff_duration:?}"
                );

                sleep(backoff_duration).await;

                match recv_loop.restart_process(&mut pending_rx).await {
                    Ok(()) => {
                        tracing::info!(
                            "Process restart {restart_count}/{max_restart_attempts} successful after error"
                        );
                        backoff_duration = std::cmp::min(backoff_duration * 2, MAX_BACKOFF);

                        // Notify restart completion
                        if let Err(e) = restart_counter_tx.send(restart_count) {
                            tracing::warn!("Failed to send restart counter: {e:?}");
                        }
                        continue;
                    }
                    Err(restart_error) => {
                        tracing::error!(
                            "Process restart {restart_count}/{max_restart_attempts} failed after error: {restart_error:?}"
                        );
                        backoff_duration = std::cmp::min(backoff_duration * 2, MAX_BACKOFF);
                        continue;
                    }
                }
            }
        }
    }

    Ok(())
}
