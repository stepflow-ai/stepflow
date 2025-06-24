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
use uuid::Uuid;

use crate::stdio::{Result, StdioError};
use crate::{IncomingHandlerRegistry, incoming::OwnedIncoming};

use super::launcher::Launcher;

struct ReceiveMessageLoop {
    child: Child,
    to_child: ChildStdin,
    from_child_stdout: LinesStream<BufReader<ChildStdout>>,
    from_child_stderr: LinesStream<BufReader<ChildStderr>>,
    pending_requests: HashMap<Uuid, oneshot::Sender<OwnedIncoming>>,
    outgoing_tx: mpsc::Sender<String>,
}

impl ReceiveMessageLoop {
    fn try_new(launcher: Launcher, outgoing_tx: mpsc::Sender<String>) -> Result<Self> {
        let mut child = launcher.spawn()?;

        let to_child = child.stdin.take().expect("stdin requested");
        let from_child_stdout = child.stdout.take().expect("stdout requested");
        
        // Use BufReader with default capacity - it will automatically handle large messages
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

    async fn iteration(
        &mut self,
        outgoing_rx: &mut mpsc::Receiver<String>,
        pending_rx: &mut mpsc::Receiver<(Uuid, oneshot::Sender<OwnedIncoming>)>,
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
                tracing::info!("Sending message to child: {outgoing}");
                self.to_child.write_all(outgoing.as_bytes()).await.change_context(StdioError::Send)?;
                self.to_child.write_all(b"\n").await.change_context(StdioError::Send)?;
                Ok(true)
            }
            Some(stderr_line) = self.from_child_stderr.next() => {
                let stderr_line = stderr_line.change_context(StdioError::Recv)?;
                tracing::info!("Component stderr: {stderr_line}");
                Ok(true)
            }
            Some(line) = self.from_child_stdout.next() => {
                let line = line.change_context(StdioError::Recv)?;
                
                // Check if the line is suspiciously long (might indicate truncation)
                if line.len() > 1024 * 1024 {
                    tracing::warn!("Received very long message ({} chars), may be truncated", line.len());
                }
                
                tracing::info!("Received line from child: {line:?}");
                
                // Add better error handling for JSON parsing
                let msg = match OwnedIncoming::try_new(line.clone()) {
                    Ok(msg) => msg,
                    Err(e) => {
                        tracing::error!("Failed to parse JSON message: {}", e);
                        tracing::error!("Message length: {} characters", line.len());
                        
                        // Check if the JSON appears to be truncated
                        if !line.trim().ends_with('}') {
                            tracing::error!("JSON appears to be truncated - doesn't end with '}}'");
                        }
                        
                        // Check for common JSON syntax issues
                        let open_braces = line.chars().filter(|&c| c == '{').count();
                        let close_braces = line.chars().filter(|&c| c == '}').count();
                        if open_braces != close_braces {
                            tracing::error!("JSON brace mismatch: {} opening, {} closing", open_braces, close_braces);
                        }
                        
                        if line.len() > 1000 {
                            tracing::error!("Message preview (first 500 chars): {}", &line[..500]);
                            tracing::error!("Message preview (last 500 chars): {}", &line[line.len()-500..]);
                        } else {
                            tracing::error!("Full message: {}", line);
                        }
                        
                        // Instead of returning an error and terminating the loop,
                        // log the error and continue processing other messages
                        tracing::warn!("Skipping malformed message and continuing...");
                        return Ok(true);
                    }
                };
                
                match (msg.method, msg.params, msg.id) {
                    (Some(method), Some(params), _) => {
                        // Incoming method call or notification.
                        // Convert to owned values for spawning
                        let method_owned = method.to_string();
                        let params_owned: Box<serde_json::value::RawValue> = params.to_owned();

                        // Handle the incoming method call
                        tracing::info!("Received incoming method call: {} with params: {:?}", method_owned, params_owned);
                        IncomingHandlerRegistry::instance().spawn_handle_incoming(method_owned, params_owned, msg.id, self.outgoing_tx.clone(), context.clone());
                        Ok(true)
                    }
                    (None, None, Some(id)) => {
                        // This is a method response.
                        // This has an ID, so it's a method response
                        if let Some(pending) = self.pending_requests.remove(&id) {
                            // We've already seen the pending request, so send the result.
                            pending.send(msg).map_err(|_| StdioError::Send)?;
                            Ok(true)
                        } else {
                            // We haven't seen the pending request, so we'll receive from
                            // the pending_rx channel until we find it.
                            loop {
                                match pending_rx.try_recv() {
                                    Ok((pending_id, pending_request)) => {
                                        if pending_id == id {
                                            pending_request.send(msg).map_err(|_| StdioError::Send)?;
                                            return Ok(true);
                                        }
                                        self.pending_requests.insert(pending_id, pending_request);
                                    }
                                    Err(TryRecvError::Empty) => {
                                        // No more pending requests. This means the response we got
                                        // is unexpected. We'll log it and move on.
                                        tracing::warn!("Unexpected response: {msg:?}");
                                        return Ok(true);
                                    }
                                    Err(TryRecvError::Disconnected) => {
                                        // The pending_rx channel is closed, so we'll exit.
                                        tracing::warn!("Pending channel is closed.")
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        tracing::warn!("Received message invalid message: {:?}", msg);
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
}

pub async fn recv_message_loop(
    launcher: Launcher,
    outgoing_tx: mpsc::Sender<String>,
    mut outgoing_rx: mpsc::Receiver<String>,
    mut pending_rx: mpsc::Receiver<(Uuid, oneshot::Sender<OwnedIncoming>)>,
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
