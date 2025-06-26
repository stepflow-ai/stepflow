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
use tracing::info;
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
                eprintln!("[component stderr] {stderr_line}");
                Ok(true)
            }
            Some(line) = self.from_child_stdout.next() => {
                let line = line.change_context(StdioError::Recv)?;
                
                // Check if the line is suspiciously long (might indicate truncation)
                if line.len() > 1024 * 1024 {
                    tracing::warn!("Received very long message ({} chars), may be truncated", line.len());
                }
                
                tracing::info!("Received line from child");
                
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
                        
                        // Try to handle concatenated JSON messages
                        if open_braces > 1 && close_braces > 1 {
                            tracing::info!("Attempting to split concatenated JSON messages");
                            let mut current_pos = 0;
                            let mut brace_count = 0;
                            let mut start_pos = 0;
                            let mut messages_parsed = 0;
                            
                            for (i, ch) in line.chars().enumerate() {
                                if ch == '{' {
                                    if brace_count == 0 {
                                        start_pos = i;
                                    }
                                    brace_count += 1;
                                } else if ch == '}' {
                                    brace_count -= 1;
                                    if brace_count == 0 {
                                        // We have a complete JSON object
                                        let json_str = &line[start_pos..=i];
                                        match OwnedIncoming::try_new(json_str.to_string()) {
                                            Ok(msg) => {
                                                tracing::info!("Successfully parsed concatenated message #{}", messages_parsed + 1);
                                                messages_parsed += 1;
                                                
                                                // Process this message
                                                match (msg.method, msg.params, msg.id) {
                                                    (Some(method), Some(params), _) => {
                                                        let method_owned = method.to_string();
                                                        let params_owned: Box<serde_json::value::RawValue> = params.to_owned();
                                                        
                                                        if let Ok(v)=serde_json::from_str::<serde_json::Value>(params_owned.get()){info!(method=%method_owned,request_id=%v["request_id"].as_str().unwrap_or("<unknown>"),stream_id=%v["stream_id"].as_str().unwrap_or("<unknown>"),chunk_index=v["chunk_index"].as_u64().unwrap_or_default(),is_final=v["is_final"].as_bool().unwrap_or(false),output_file=%v["output_file"].as_str().unwrap_or("<unknown>"),"Received incoming method call");}
                                                        IncomingHandlerRegistry::instance().spawn_handle_incoming(method_owned, params_owned, msg.id, self.outgoing_tx.clone(), context.clone());
                                                    }
                                                    (None, None, Some(id)) => {
                                                        // Handle method response
                                                        if let Some(pending) = self.pending_requests.remove(&id) {
                                                            pending.send(msg).map_err(|_| StdioError::Send)?;
                                                        }
                                                    }
                                                    _ => {
                                                        tracing::warn!("Received invalid concatenated message: {:?}", msg);
                                                    }
                                                }
                                            }
                                            Err(parse_err) => {
                                                tracing::warn!("Failed to parse concatenated JSON message: {}", parse_err);
                                            }
                                        }
                                    }
                                }
                            }
                            
                            if messages_parsed > 0 {
                                tracing::info!("Successfully parsed {} concatenated messages", messages_parsed);
                                return Ok(true);
                            }
                        }
                        
                        // Try more aggressive message recovery for edge cases
                        tracing::info!("Attempting aggressive message recovery");
                        let mut messages_parsed = 0;
                        
                        // Try to find valid JSON objects by looking for complete message patterns
                        let mut pos = 0;
                        while pos < line.len() {
                            // Find the start of a potential JSON object
                            if let Some(start) = line[pos..].find("{\"jsonrpc\":\"2.0\"") {
                                let start_pos = pos + start;
                                
                                // Try to find the matching closing brace
                                let mut brace_count = 0;
                                let mut end_pos = start_pos;
                                let mut found_end = false;
                                
                                for (i, ch) in line[start_pos..].chars().enumerate() {
                                    if ch == '{' {
                                        brace_count += 1;
                                    } else if ch == '}' {
                                        brace_count -= 1;
                                        if brace_count == 0 {
                                            end_pos = start_pos + i;
                                            found_end = true;
                                            break;
                                        }
                                    }
                                }
                                
                                if found_end {
                                    let json_str = &line[start_pos..=end_pos];
                                    match OwnedIncoming::try_new(json_str.to_string()) {
                                        Ok(msg) => {
                                            tracing::info!("Successfully recovered message #{}", messages_parsed + 1);
                                            messages_parsed += 1;
                                            
                                            // Process this message
                                            match (msg.method, msg.params, msg.id) {
                                                (Some(method), Some(params), _) => {
                                                    let method_owned = method.to_string();
                                                    let params_owned: Box<serde_json::value::RawValue> = params.to_owned();
                                                    
                                                    if let Ok(v)=serde_json::from_str::<serde_json::Value>(params_owned.get()){info!(method=%method_owned,request_id=%v["request_id"].as_str().unwrap_or("<unknown>"),stream_id=%v["stream_id"].as_str().unwrap_or("<unknown>"),chunk_index=v["chunk_index"].as_u64().unwrap_or_default(),is_final=v["is_final"].as_bool().unwrap_or(false),output_file=%v["output_file"].as_str().unwrap_or("<unknown>"),"Received incoming method call");}
                                                    IncomingHandlerRegistry::instance().spawn_handle_incoming(method_owned, params_owned, msg.id, self.outgoing_tx.clone(), context.clone());
                                                }
                                                (None, None, Some(id)) => {
                                                    // Handle method response
                                                    if let Some(pending) = self.pending_requests.remove(&id) {
                                                        pending.send(msg).map_err(|_| StdioError::Send)?;
                                                    }
                                                }
                                                _ => {
                                                    tracing::warn!("Received invalid recovered message: {:?}", msg);
                                                }
                                            }
                                            
                                            // Move position to after this message
                                            pos = end_pos + 1;
                                        }
                                        Err(parse_err) => {
                                            tracing::warn!("Failed to parse recovered JSON message: {}", parse_err);
                                            pos = start_pos + 1;
                                        }
                                    }
                                } else {
                                    // No matching end found, move to next potential start
                                    pos = start_pos + 1;
                                }
                            } else {
                                // No more JSON objects found
                                break;
                            }
                        }
                        
                        if messages_parsed > 0 {
                            tracing::info!("Successfully recovered {} messages", messages_parsed);
                            return Ok(true);
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
                        if let Ok(v)=serde_json::from_str::<serde_json::Value>(params_owned.get()){info!(method=%method_owned,request_id=%v["request_id"].as_str().unwrap_or("<unknown>"),stream_id=%v["stream_id"].as_str().unwrap_or("<unknown>"),chunk_index=v["chunk_index"].as_u64().unwrap_or_default(),is_final=v["is_final"].as_bool().unwrap_or(false),output_file=%v["output_file"].as_str().unwrap_or("<unknown>"),"Received incoming method call");}
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
