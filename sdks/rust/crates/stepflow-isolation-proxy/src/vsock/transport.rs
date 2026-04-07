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
//! Supports two connection modes:
//! - **Direct Unix socket** (dev/subprocess mode): plain `UnixStream::connect`
//! - **Firecracker vsock** (VM mode): connect to vsock UDS, send `CONNECT <port>\n`,
//!   read `OK <port>\n` response, then use the socket as a data pipe.

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// A connection to a worker over vsock or Unix socket.
pub struct VsockConnection {
    pub stream: UnixStream,
}

impl VsockConnection {
    /// Connect to a worker via a plain Unix domain socket (dev/subprocess mode).
    pub async fn connect(path: &str) -> std::io::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Self { stream })
    }

    /// Connect to a worker inside a Firecracker VM via the vsock UDS.
    ///
    /// Firecracker's host-initiated vsock protocol:
    /// 1. Connect to the vsock UDS path
    /// 2. Send `CONNECT <guest_port>\n`
    /// 3. Receive `OK <host_port>\n`
    /// 4. Connection is now a data pipe to the guest
    pub async fn connect_vsock(uds_path: &str, guest_port: u32) -> std::io::Result<Self> {
        let stream = UnixStream::connect(uds_path).await?;

        // We need to split into read/write halves for the handshake,
        // then reconstruct the stream for data transfer.
        let (reader, mut writer) = tokio::io::split(stream);

        // Send CONNECT handshake
        let connect_msg = format!("CONNECT {guest_port}\n");
        writer.write_all(connect_msg.as_bytes()).await?;
        writer.flush().await?;

        // Read response line
        let mut buf_reader = BufReader::new(reader);
        let mut response = String::new();
        buf_reader.read_line(&mut response).await?;

        if !response.starts_with("OK") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!(
                    "Firecracker vsock CONNECT {guest_port} failed: {}",
                    response.trim()
                ),
            ));
        }

        // Reunite the split halves back into a UnixStream
        let reader = buf_reader.into_inner();
        let stream = reader.unsplit(writer);

        Ok(Self { stream })
    }
}
