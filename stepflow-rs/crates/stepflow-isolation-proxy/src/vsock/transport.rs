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

//! Transport abstraction over vsock and Unix sockets.
//!
//! In production (Firecracker), the proxy connects to the VM via a vsock UDS path
//! exposed by the Firecracker process. In dev mode (no VM), it connects to a local
//! Python worker via a regular Unix socket.

use tokio::net::UnixStream;

/// A connection to a worker over vsock or Unix socket.
///
/// In both cases, Firecracker exposes vsock as a Unix domain socket on the host,
/// so `UnixStream` is the underlying transport for both modes.
pub struct VsockConnection {
    pub stream: UnixStream,
}

impl VsockConnection {
    /// Connect to a worker via Unix domain socket path.
    ///
    /// For Firecracker VMs, this is the vsock UDS path (e.g., `/tmp/fc-{id}.vsock`).
    /// For dev mode, this is a regular Unix socket path.
    pub async fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Self { stream })
    }
}
