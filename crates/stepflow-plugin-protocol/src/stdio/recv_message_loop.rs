use std::{collections::HashMap, ffi::OsString, path::PathBuf, process::Stdio};

use error_stack::ResultExt as _;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::{
        mpsc::{self, error::TryRecvError},
        oneshot,
    },
};
use tokio_stream::{StreamExt as _, wrappers::LinesStream};
use uuid::Uuid;

use crate::incoming::OwnedIncoming;
use crate::stdio::{Result, StdioError};

struct ReceiveMessageLoop {
    child: Child,
    to_child: ChildStdin,
    from_child: LinesStream<BufReader<ChildStdout>>,
    pending_requests: HashMap<Uuid, oneshot::Sender<OwnedIncoming>>,
}

impl ReceiveMessageLoop {
    fn try_new(command: PathBuf, args: Vec<OsString>) -> Result<Self> {
        let child = tokio::process::Command::new(&command)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn();

        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                tracing::error!(
                    "Failed to spawn child process '{} {args:?}': {e}",
                    command.display()
                );
                return Err(StdioError::Spawn.into());
            }
        };

        let to_child = child.stdin.take().expect("stdin requested");
        let from_child = child.stdout.take().expect("stdout requested");
        let from_child = LinesStream::new(BufReader::new(from_child).lines());

        Ok(Self {
            child,
            to_child,
            from_child,
            pending_requests: HashMap::new(),
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
            Some(line) = self.from_child.next() => {
                let line = line.change_context(StdioError::Recv)?;
                tracing::info!("Received line from child: {line:?}");
                let msg = OwnedIncoming::try_new(line).change_context(StdioError::Recv)?;
                if let Some(id) = msg.id.as_ref() {
                    // This has an ID, so it's a method response.
                    if let Some(pending) = self.pending_requests.remove(id) {
                        // We've already seen the pending request, so send the result.
                        pending.send(msg).map_err(|_| StdioError::Send)?;
                        Ok(true)
                    } else {
                        // We haven't seen the pending request, so we'll receive from
                        // the pending_rx channel until we find it.
                        loop {
                            match pending_rx.try_recv() {
                                Ok((pending_id, pending_request)) => {
                                    if pending_id == *id {
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
                } else {
                    // This is a notification.
                    todo!("handle notification {msg:?}");
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
    command: PathBuf,
    args: Vec<OsString>,
    mut outgoing_rx: mpsc::Receiver<String>,
    mut pending_rx: mpsc::Receiver<(Uuid, oneshot::Sender<OwnedIncoming>)>,
) -> Result<()> {
    let mut recv_loop = ReceiveMessageLoop::try_new(command, args)?;

    loop {
        match recv_loop.iteration(&mut outgoing_rx, &mut pending_rx).await {
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
