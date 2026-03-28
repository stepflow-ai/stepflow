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

//! Pre-warm pool of Firecracker VMs.
//!
//! Maintains a pool of ready-to-use VMs restored from a base snapshot.
//! VMs are created ahead of tasks so the restore latency is off the
//! critical path.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use log::{error, info, warn};
use tokio::sync::{Mutex, mpsc};

use super::lifecycle::{FirecrackerVm, VmConfig, VmError};
use super::snapshot::BaseSnapshot;

/// A pool of pre-warmed Firecracker VMs.
pub struct VmPool {
    config: VmConfig,
    snapshot: Option<BaseSnapshot>,
    ready: Arc<Mutex<Vec<FirecrackerVm>>>,
    #[allow(dead_code)]
    target_size: usize,
    /// Channel to request more VMs to be warmed.
    warm_tx: mpsc::Sender<()>,
}

impl VmPool {
    /// Create a new VM pool.
    ///
    /// If `create_snapshot` is true, boots a base VM and snapshots it first.
    /// Otherwise, VMs are booted from scratch (slower).
    pub async fn new(
        config: VmConfig,
        #[allow(dead_code)] target_size: usize,
        create_snapshot: bool,
    ) -> Result<Self, VmError> {
        let snapshot = if create_snapshot {
            info!("Creating base snapshot for VM pool...");
            match BaseSnapshot::create(&config).await {
                Ok(snap) => {
                    info!("Base snapshot ready at {}", snap.dir.display());
                    Some(snap)
                }
                Err(e) => {
                    warn!("Failed to create snapshot, falling back to full boot: {e}");
                    None
                }
            }
        } else {
            None
        };

        let ready = Arc::new(Mutex::new(Vec::new()));
        let (warm_tx, warm_rx) = mpsc::channel((target_size * 2).max(1));

        let pool = Self {
            config: config.clone(),
            snapshot,
            ready: Arc::clone(&ready),
            target_size,
            warm_tx,
        };

        // Start the background warming loop
        let snapshot_dir = pool.snapshot.as_ref().map(|s| s.dir.clone());
        tokio::spawn(warm_loop(config, snapshot_dir, ready, target_size, warm_rx));

        // Trigger initial warming
        for _ in 0..target_size {
            let _ = pool.warm_tx.send(()).await;
        }

        Ok(pool)
    }

    /// Take a ready VM from the pool (blocks until one is available).
    pub async fn take(&self) -> Result<FirecrackerVm, VmError> {
        let maybe_vm = {
            let mut ready = self.ready.lock().await;
            ready.pop()
        }; // Lock dropped before any awaits

        if let Some(vm) = maybe_vm {
            info!("Pool: took VM {} (requesting replacement)", vm.vm_id);
            if self.warm_tx.send(()).await.is_err() {
                warn!("Pool: warm channel closed, cannot request replacement");
            }
            return Ok(vm);
        }

        // No ready VMs — create one on-demand
        info!("Pool empty, creating VM on-demand...");
        self.create_vm().await
    }

    /// Create a single VM (from snapshot if available, otherwise full boot).
    async fn create_vm(&self) -> Result<FirecrackerVm, VmError> {
        let start = Instant::now();
        let vm = if let Some(ref snapshot) = self.snapshot {
            FirecrackerVm::restore(&snapshot.dir, &self.config).await?
        } else {
            FirecrackerVm::boot(&self.config).await?
        };
        info!("VM {} created in {:?}", vm.vm_id, start.elapsed());
        Ok(vm)
    }

    /// Number of ready VMs in the pool.
    #[allow(dead_code)]
    pub async fn ready_count(&self) -> usize {
        self.ready.lock().await.len()
    }

    /// Whether the pool has a base snapshot.
    #[allow(dead_code)]
    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }
}

/// Background loop that keeps the pool filled to target_size.
async fn warm_loop(
    config: VmConfig,
    snapshot_dir: Option<PathBuf>,
    ready: Arc<Mutex<Vec<FirecrackerVm>>>,
    #[allow(dead_code)] target_size: usize,
    mut rx: mpsc::Receiver<()>,
) {
    while rx.recv().await.is_some() {
        // Check if we need more VMs
        let current = ready.lock().await.len();
        if current >= target_size {
            continue;
        }

        let start = Instant::now();
        let result = if let Some(ref dir) = snapshot_dir {
            FirecrackerVm::restore(dir, &config).await
        } else {
            FirecrackerVm::boot(&config).await
        };

        match result {
            Ok(vm) => {
                info!("Pool: warmed VM {} in {:?}", vm.vm_id, start.elapsed());
                ready.lock().await.push(vm);
            }
            Err(e) => {
                error!("Pool: failed to warm VM: {e}");
            }
        }
    }
}
