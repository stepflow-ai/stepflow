use std::{collections::HashMap, ffi::OsString, path::PathBuf, process::Stdio};

use error_stack::ResultExt as _;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
    sync::{
        mpsc::{self, error::TryRecvError},
        oneshot,
    },
};
use tokio_stream::{StreamExt as _, wrappers::LinesStream};
use uuid::Uuid;

use crate::incoming::OwnedIncoming;
use crate::stdio::{Result, StdioError};

pub async fn recv_message_loop(
    command: PathBuf,
    args: Vec<OsString>,
    mut outgoing_rx: mpsc::Receiver<String>,
    mut pending_rx: mpsc::Receiver<(Uuid, oneshot::Sender<OwnedIncoming>)>,
) -> Result<()> {
    // Start the child process.
    let mut child = tokio::process::Command::new(&command)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .change_context(StdioError::Spawn)?;

    let mut to_child = child.stdin.take().expect("stdin requested");
    let from_child = child.stdout.take().expect("stdout requested");
    let mut from_child = LinesStream::new(BufReader::new(from_child).lines());

    // TODO: Restart the child process if it exits abruptly? Or retry the flow from that point?

    // Start the message loop waiting for messages from the child (on from_child)
    // and lines to send to the child (on outgoing_rx).

    let mut pending_requests: HashMap<Uuid, oneshot::Sender<OwnedIncoming>> = HashMap::new();

    loop {
        tokio::select! {
            child = child.wait() => {
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
                break;
            }
            Some(outgoing) = outgoing_rx.recv() => {
                to_child.write_all(outgoing.as_bytes()).await.change_context(StdioError::Send)?;
                to_child.write_all(b"\n").await.change_context(StdioError::Send)?;
            }
            Some(line) = from_child.next() => {
                let line = line.change_context(StdioError::Recv)?;
                tracing::trace!("Received line: {line}");
                let msg = OwnedIncoming::try_new(line).change_context(StdioError::Recv)?;
                if let Some(id) = msg.id.as_ref() {
                    // This has an ID, so it's a method response.
                    if let Some(pending) = pending_requests.remove(id) {
                        // We've already seen the pending request, so send the result.
                        pending.send(msg).map_err(|_| StdioError::Send)?;
                        continue;
                    } else {
                        // We haven't seen the pending request, so we'll receive from
                        // the pending_rx channel until we find it.
                        loop {
                            match pending_rx.try_recv() {
                                Ok((pending_id, pending_request)) => {
                                    if pending_id == *id {
                                        pending_request.send(msg).map_err(|_| StdioError::Send)?;
                                        break;
                                    }
                                    pending_requests.insert(pending_id, pending_request);
                                }
                                Err(TryRecvError::Empty) => {
                                    // No more pending requests. This means the response we got
                                    // is unexpected. We'll log it and move on.
                                    tracing::warn!("Unexpected response: {msg:?}");
                                    break;
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
                break;
            }
        }
    }

    Ok(())
}
