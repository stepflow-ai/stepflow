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

//! Firecracker REST API client over Unix socket.
//!
//! Firecracker exposes a REST API on a Unix domain socket. This module
//! provides typed helpers for the API calls needed to boot, configure,
//! snapshot, and control VMs.

use std::path::Path;

use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper::body::Bytes;
use log::debug;
use serde_json::json;

/// Error from a Firecracker API call.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] hyper::Error),
    #[error("Firecracker API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Client for the Firecracker REST API over Unix socket.
pub struct FirecrackerApi {
    socket_path: String,
}

impl FirecrackerApi {
    pub fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
        }
    }

    /// PUT /boot-source — configure the kernel.
    pub async fn set_boot_source(
        &self,
        kernel_path: &Path,
        boot_args: &str,
    ) -> Result<(), ApiError> {
        let body = json!({
            "kernel_image_path": kernel_path.to_str().unwrap(),
            "boot_args": boot_args,
        });
        self.put("/boot-source", &body).await
    }

    /// PUT /drives/rootfs — attach the root filesystem.
    pub async fn set_rootfs(&self, rootfs_path: &Path) -> Result<(), ApiError> {
        let body = json!({
            "drive_id": "rootfs",
            "path_on_host": rootfs_path.to_str().unwrap(),
            "is_root_device": true,
            "is_read_only": false,
        });
        self.put("/drives/rootfs", &body).await
    }

    /// PUT /machine-config — set CPU and memory.
    pub async fn set_machine_config(
        &self,
        vcpu_count: u32,
        mem_size_mib: u32,
    ) -> Result<(), ApiError> {
        let body = json!({
            "vcpu_count": vcpu_count,
            "mem_size_mib": mem_size_mib,
            "smt": false,
        });
        self.put("/machine-config", &body).await
    }

    /// PUT /network-interfaces/eth0 — attach a TAP network device.
    pub async fn set_network_interface(
        &self,
        iface_id: &str,
        host_dev_name: &str,
        guest_mac: &str,
    ) -> Result<(), ApiError> {
        let body = json!({
            "iface_id": iface_id,
            "host_dev_name": host_dev_name,
            "guest_mac": guest_mac,
        });
        self.put(&format!("/network-interfaces/{iface_id}"), &body)
            .await
    }

    /// PUT /vsock — attach a vsock device.
    pub async fn set_vsock(&self, guest_cid: u32, uds_path: &str) -> Result<(), ApiError> {
        let body = json!({
            "vsock_id": "vsock0",
            "guest_cid": guest_cid,
            "uds_path": uds_path,
        });
        self.put("/vsock", &body).await
    }

    /// PUT /actions — start the VM instance.
    pub async fn instance_start(&self) -> Result<(), ApiError> {
        let body = json!({ "action_type": "InstanceStart" });
        self.put("/actions", &body).await
    }

    /// PATCH /vm — pause or resume the VM.
    pub async fn set_vm_state(&self, state: &str) -> Result<(), ApiError> {
        let body = json!({ "state": state });
        self.patch("/vm", &body).await
    }

    /// PUT /snapshot/create — create a snapshot.
    pub async fn create_snapshot(
        &self,
        snapshot_path: &str,
        mem_file_path: &str,
    ) -> Result<(), ApiError> {
        let body = json!({
            "snapshot_type": "Full",
            "snapshot_path": snapshot_path,
            "mem_file_path": mem_file_path,
        });
        self.put("/snapshot/create", &body).await
    }

    /// PUT /snapshot/load — restore from a snapshot.
    pub async fn load_snapshot(
        &self,
        snapshot_path: &str,
        mem_file_path: &str,
    ) -> Result<(), ApiError> {
        let body = json!({
            "snapshot_path": snapshot_path,
            "mem_backend": {
                "backend_type": "File",
                "backend_path": mem_file_path,
            },
            "enable_diff_snapshots": false,
            "resume_vm": false,
        });
        self.put("/snapshot/load", &body).await
    }

    // --- Internal helpers ---

    async fn put(&self, path: &str, body: &serde_json::Value) -> Result<(), ApiError> {
        self.request("PUT", path, body).await
    }

    async fn patch(&self, path: &str, body: &serde_json::Value) -> Result<(), ApiError> {
        self.request("PATCH", path, body).await
    }

    async fn request(
        &self,
        method: &str,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let body_bytes = serde_json::to_vec(body)?;
        debug!(
            "Firecracker API: {method} {path} ({} bytes)",
            body_bytes.len()
        );

        let socket_path = self.socket_path.clone();
        let stream = tokio::net::UnixStream::connect(&socket_path)
            .await
            .map_err(|e| ApiError::Connection(format!("connect to {socket_path}: {e}")))?;

        let io = hyper_util::rt::TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| ApiError::Connection(format!("handshake: {e}")))?;

        // Drive the connection in the background
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                log::warn!("Firecracker API connection error: {e}");
            }
        });

        let req = Request::builder()
            .method(method)
            .uri(format!("http://localhost{path}"))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(Full::new(Bytes::from(body_bytes)))
            .map_err(|e| ApiError::Connection(format!("build request: {e}")))?;

        let resp = sender
            .send_request(req)
            .await
            .map_err(|e| ApiError::Connection(format!("send request: {e}")))?;

        let status = resp.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            let body = resp
                .into_body()
                .collect()
                .await
                .map(|b| String::from_utf8_lossy(&b.to_bytes()).to_string())
                .unwrap_or_else(|_| "<could not read body>".to_string());
            Err(ApiError::Api { status, body })
        }
    }
}
