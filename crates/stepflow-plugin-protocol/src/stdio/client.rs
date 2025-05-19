use std::{ffi::OsString, path::PathBuf};

use error_stack::ResultExt as _;
use serde::de::DeserializeOwned;

use stepflow_protocol::{Method, Notification, Request};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::Instrument as _;
use uuid::Uuid;

use super::recv_message_loop::recv_message_loop;
use crate::incoming::OwnedIncoming;
use crate::stdio::{Result, StdioError};

/// Manages a client process spawned from a command.
///
/// Messages may be sent (as lines) to a channel that are sent via stdio.
pub struct Client {
    outgoing_tx: mpsc::Sender<String>,
    pending_tx: mpsc::Sender<(Uuid, oneshot::Sender<OwnedIncoming>)>,
    // TODO: Use the handle. We should actually check it for errors
    // before bubbling other (less meaningful) errors up.
    #[allow(dead_code)]
    loop_handle: JoinHandle<Result<()>>,
}

impl Client {
    pub fn builder(command: impl Into<PathBuf>) -> Builder {
        Builder::new(command)
    }

    pub async fn try_new(command: PathBuf, args: Vec<OsString>) -> Result<Self> {
        error_stack::ensure!(command.is_file(), StdioError::InvalidCommand(command));

        let (outgoing_tx, outgoing_rx) = mpsc::channel(100);
        let (pending_tx, pending_rx) = mpsc::channel(100);

        let recv_span = tracing::info_span!("recv_message_loop", command = ?command, args = ?args);
        let loop_handle = tokio::spawn(
            recv_message_loop(command, args, outgoing_rx, pending_rx).instrument(recv_span),
        );

        Ok(Self {
            outgoing_tx,
            pending_tx,
            loop_handle,
        })
    }

    pub fn handle(&self) -> ClientHandle {
        ClientHandle {
            outgoing_tx: self.outgoing_tx.clone(),
            pending_tx: self.pending_tx.clone(),
        }
    }
}

pub struct ClientHandle {
    outgoing_tx: mpsc::Sender<String>,
    pending_tx: mpsc::Sender<(Uuid, oneshot::Sender<OwnedIncoming>)>,
}

impl ClientHandle {
    pub async fn request<I>(&self, params: &I) -> Result<I::Response>
    where
        I: Method + serde::Serialize + Send + Sync,
        I::Response: DeserializeOwned + Send + Sync + 'static,
    {
        let response = self.request_dyn(I::METHOD_NAME, params).await?;
        let raw_result = response.raw_value()?;
        let result: I::Response =
            serde_json::from_str(raw_result.get()).change_context(StdioError::InvalidResponse)?;
        Ok(result)
    }

    pub async fn notify<I>(&self, params: &I) -> Result<()>
    where
        I: Notification + serde::Serialize + Send + Sync,
    {
        self.send(&Request {
            jsonrpc: "2.0",
            id: None,
            method: I::NOTIFICATION_NAME,
            params,
        })
        .await?;

        Ok(())
    }

    async fn send(&self, msg: &(dyn erased_serde::Serialize + Send + Sync)) -> Result<()> {
        let msg = serde_json::to_string(&msg).change_context(StdioError::Send)?;

        self.outgoing_tx
            .send(msg)
            .await
            .change_context(StdioError::Send)?;

        Ok(())
    }

    async fn request_dyn(
        &self,
        method: &str,
        params: &(dyn erased_serde::Serialize + Send + Sync),
    ) -> Result<OwnedIncoming> {
        let id = Uuid::new_v4();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.pending_tx
            .send((id, response_tx))
            .await
            .map_err(|_| StdioError::Send)?;

        let request = Request {
            jsonrpc: "2.0",
            id: Some(id),
            method,
            params,
        };
        self.send(&request).await?;

        let response = response_rx.await.change_context(StdioError::Recv)?;
        debug_assert_eq!(response.id, Some(id));
        Ok(response)
    }
}

pub struct Builder {
    command: PathBuf,
    args: Vec<OsString>,
}

impl Builder {
    pub(crate) fn new(command: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I: Into<OsString>>(mut self, args: impl IntoIterator<Item = I>) -> Self {
        for arg in args {
            self.args.push(arg.into());
        }
        self
    }

    pub async fn build(self) -> Result<Client> {
        let client = Client::try_new(self.command, self.args).await?;
        Ok(client)
    }
}
