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

//! Firecracker VM snapshot creation using the jailer.
//!
//! Creates a base snapshot by booting a jailed VM, waiting for the vsock worker
//! to be ready, then pausing and snapshotting. The snapshot files (vmstate,
//! memory, rootfs) are copied out of the jail for use by restore.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use log::{debug, info};

use super::lifecycle::{VmConfig, VmError};

/// A base snapshot that can be used to quickly clone VMs.
pub struct BaseSnapshot {
    pub dir: PathBuf,
}

impl BaseSnapshot {
    /// Create a base snapshot by booting a VM via the jailer and snapshotting it.
    pub async fn create(config: &VmConfig) -> Result<Self, VmError> {
        let snapshot_id = format!(
            "snapshot-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>()
        );
        let snapshot_output_dir =
            PathBuf::from(format!("/tmp/firecracker-snapshots/{snapshot_id}"));
        tokio::fs::create_dir_all(&snapshot_output_dir).await?;

        info!("Creating base snapshot...");
        let start = Instant::now();

        // Boot a VM using the standard boot path
        let vm = super::lifecycle::FirecrackerVm::boot(config).await?;
        debug!("Snapshot VM booted in {:?}", start.elapsed());

        // Wait for the guest vsock worker to be ready.
        // The VM needs ~7-10s to boot Alpine and start the Python worker.
        info!("Waiting for guest vsock worker to be ready...");
        let mut guest_ready = false;
        for _attempt in 1..=30 {
            match crate::vsock::transport::VsockConnection::connect_vsock(&vm.vsock_uds_path, 5000)
                .await
            {
                Ok(_conn) => {
                    guest_ready = true;
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        if !guest_ready {
            // Clean up and return error
            vm.shutdown().await;
            return Err(VmError::Timeout(
                "Guest vsock worker not ready after 30s".to_string(),
            ));
        }
        info!("Guest worker ready in {:?}", start.elapsed());

        // Re-chmod the API socket (permissions may have changed during boot)
        super::lifecycle::sudo(&["chmod", "666", &vm.api_socket_path]).await;

        // Pause the VM
        let api = super::api::FirecrackerApi::new(&vm.api_socket_path);
        api.set_vm_state("Paused").await?;

        // Create snapshot inside the jail
        api.create_snapshot("vmstate", "memory").await?;

        // Copy snapshot files out of the jail
        let jail_root = &vm.jail_root;
        for filename in ["vmstate", "memory", "rootfs.ext4"] {
            super::lifecycle::sudo(&[
                "cp",
                jail_root.join(filename).to_str().unwrap(),
                snapshot_output_dir.join(filename).to_str().unwrap(),
            ])
            .await;
        }

        info!(
            "Base snapshot created in {:?} at {}",
            start.elapsed(),
            snapshot_output_dir.display()
        );

        // Clean up the snapshot VM
        vm.shutdown().await;

        Ok(Self {
            dir: snapshot_output_dir,
        })
    }
}
