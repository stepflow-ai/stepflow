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

//! Length-delimited protobuf framing for async byte streams.
//!
//! Provides generic read/write functions for sending protobuf messages over
//! any async byte stream (vsock, Unix socket, TCP, etc.).
//!
//! Wire format: `[4-byte big-endian length][protobuf bytes]`
//!
//! Messages are sequential on a single connection. End-of-stream (EOF)
//! signals no more messages.
//!
//! # Usage
//!
//! Used by isolation proxies (Firecracker, gVisor, WASM) to dispatch
//! [`VsockTaskEnvelope`](crate::VsockTaskEnvelope) messages to
//! workers running inside sandboxed environments. The worker reads
//! envelopes, executes tasks, and talks directly to the orchestrator
//! via gRPC for heartbeats, completion, and blob access.

use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// Maximum message size (16 MiB). Prevents OOM from malformed length prefixes.
const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Write a length-delimited protobuf message.
pub async fn write_message<W: AsyncWrite + Unpin, M: Message>(
    writer: &mut W,
    msg: &M,
) -> std::io::Result<()> {
    let encoded = msg.encode_to_vec();
    let len = encoded.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-delimited protobuf message.
///
/// Returns `Ok(None)` on clean EOF (end-of-stream = no more messages).
pub async fn read_message<R: AsyncRead + Unpin, M: Message + Default>(
    reader: &mut R,
) -> std::io::Result<Option<M>> {
    // Read 4-byte length prefix
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let len = u32::from_be_bytes(len_buf);
    if len > MAX_MESSAGE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message too large: {len} bytes (max {MAX_MESSAGE_SIZE})"),
        ));
    }

    // Read message body
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf).await?;

    let msg = M::decode(&buf[..]).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("protobuf decode: {e}"),
        )
    })?;

    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VsockTaskEnvelope;

    #[tokio::test]
    async fn round_trip() {
        let envelope = VsockTaskEnvelope {
            assignment: Some(crate::TaskAssignment {
                task_id: "task-123".to_string(),
                heartbeat_interval_secs: 5,
                ..Default::default()
            }),
            blob_url: "http://localhost:7840/api/v1/blobs".to_string(),
            tasks_url: "http://localhost:7837".to_string(),
            otel_endpoint: "http://otel:4317".to_string(),
        };

        let mut buf = Vec::new();
        write_message(&mut buf, &envelope).await.unwrap();

        let mut cursor = &buf[..];
        let decoded: Option<VsockTaskEnvelope> = read_message(&mut cursor).await.unwrap();
        let decoded = decoded.expect("should decode a message");

        assert_eq!(decoded.blob_url, "http://localhost:7840/api/v1/blobs");
        assert_eq!(decoded.tasks_url, "http://localhost:7837");
        assert_eq!(decoded.otel_endpoint, "http://otel:4317");
        let assignment = decoded.assignment.unwrap();
        assert_eq!(assignment.task_id, "task-123");
        assert_eq!(assignment.heartbeat_interval_secs, 5);
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let empty: &[u8] = &[];
        let mut cursor = empty;
        let result: Option<VsockTaskEnvelope> = read_message(&mut cursor).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn multiple_messages() {
        let mut buf = Vec::new();
        for i in 0..3 {
            let envelope = VsockTaskEnvelope {
                assignment: Some(crate::TaskAssignment {
                    task_id: format!("task-{i}"),
                    ..Default::default()
                }),
                ..Default::default()
            };
            write_message(&mut buf, &envelope).await.unwrap();
        }

        let mut cursor = &buf[..];
        for i in 0..3 {
            let msg: Option<VsockTaskEnvelope> = read_message(&mut cursor).await.unwrap();
            let msg = msg.expect("should have message");
            assert_eq!(msg.assignment.unwrap().task_id, format!("task-{i}"));
        }

        let msg: Option<VsockTaskEnvelope> = read_message(&mut cursor).await.unwrap();
        assert!(msg.is_none());
    }
}
