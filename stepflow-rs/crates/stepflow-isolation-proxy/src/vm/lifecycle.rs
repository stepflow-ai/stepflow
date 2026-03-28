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

//! Firecracker VM lifecycle using the jailer for isolation.
//!
//! Each VM runs in a jailer chroot at `/srv/jailer/firecracker/<id>/root/`.
//! The jailer provides:
//! - Filesystem isolation (chroot)
//! - Network namespace (--netns)
//! - PID namespace (--new-pid-ns)
//! - Privilege dropping (--uid/--gid)
//! - Vsock UDS isolation (paths are inside the chroot)

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use tokio::process::{Child, Command};

use super::api::FirecrackerApi;
use super::namespace::VmNamespace;

/// Run a sudo command, logging a warning on failure (best-effort cleanup).
pub(super) async fn sudo(args: &[&str]) {
    if let Err(e) = sudo_checked(args).await {
        warn!("{e}");
    }
}

/// Run a sudo command, returning an error on failure.
async fn sudo_checked(args: &[&str]) -> Result<(), VmError> {
    let output = Command::new("sudo")
        .args(args)
        .output()
        .await
        .map_err(|e| VmError::Process(format!("sudo {} exec failed: {e}", args.join(" "))))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::Process(format!(
            "sudo {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(())
}

/// Configuration for a Firecracker VM.
#[derive(Debug, Clone)]
pub struct VmConfig {
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub vcpu_count: u32,
    pub mem_size_mib: u32,
    pub boot_args: String,
    /// Path to the jailer binary.
    pub jailer_path: PathBuf,
    /// Path to the firecracker binary.
    pub firecracker_path: PathBuf,
    /// UID to run Firecracker as inside the jail.
    pub jail_uid: u32,
    /// GID to run Firecracker as inside the jail.
    pub jail_gid: u32,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            kernel_path: PathBuf::from("./build/vmlinux.bin"),
            rootfs_path: PathBuf::from("./build/stepflow-rootfs.ext4"),
            vcpu_count: 1,
            mem_size_mib: 256,
            boot_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
            jailer_path: PathBuf::from("/usr/local/bin/jailer"),
            firecracker_path: PathBuf::from("/usr/local/bin/firecracker"),
            jail_uid: 1000,
            jail_gid: 1000,
        }
    }
}

/// A running Firecracker VM inside a jailer chroot.
pub struct FirecrackerVm {
    pub vm_id: String,
    pub namespace: VmNamespace,
    /// Host-side path to the API socket inside the jail.
    pub api_socket_path: String,
    /// Host-side path to the vsock UDS inside the jail.
    pub vsock_uds_path: String,
    /// Host-side veth IP — the IP this VM uses to reach host services.
    pub host_ip: String,
    /// The jail's root directory on the host.
    pub jail_root: PathBuf,
    #[allow(dead_code)] // Used in shutdown() but rustc doesn't see it as "read"
    process: Child,
    #[allow(dead_code)] // For future metrics/logging
    start_time: Instant,
}

/// Error during VM operations.
#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("Namespace error: {0}")]
    Namespace(#[from] super::namespace::NamespaceError),
    #[error("Firecracker API error: {0}")]
    Api(#[from] super::api::ApiError),
    #[error("Process error: {0}")]
    Process(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl FirecrackerVm {
    /// Boot a new Firecracker VM using the jailer.
    pub async fn boot(config: &VmConfig) -> Result<Self, VmError> {
        let vm_id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();
        let start = Instant::now();

        info!("Booting VM {vm_id} via jailer");

        // 1. Create network namespace with TAP device
        let namespace = VmNamespace::create().await?;
        debug!(
            "Namespace {} created in {:?}",
            namespace.name,
            start.elapsed()
        );

        // 2. Jail root path (created by jailer on start)
        let jail_root = PathBuf::from(format!("/srv/jailer/firecracker/{vm_id}/root"));
        let api_sock_name = "firecracker.socket";
        let api_socket_path = jail_root.join(api_sock_name).to_str().unwrap().to_string();
        let vsock_name = "vsock.sock";
        let vsock_uds_path = jail_root.join(vsock_name).to_str().unwrap().to_string();

        // 3. Start jailer — creates chroot, execs Firecracker inside it
        let mut process = start_jailer(config, &vm_id, &namespace.name, api_sock_name).await?;

        // Wait for API socket (jailer + Firecracker are running now)
        // The jailer may take a moment to chroot and exec Firecracker
        debug!("Waiting for API socket at {api_socket_path}...");
        wait_for_socket(&api_socket_path, Duration::from_secs(30)).await?;
        debug!("Jailer started in {:?}", start.elapsed());

        // 4. Copy kernel and rootfs into the jail (fail early if copies fail)
        let kernel_dst = jail_root.join("vmlinux.bin");
        let rootfs_dst = jail_root.join("rootfs.ext4");
        sudo_checked(&[
            "cp",
            config.kernel_path.to_str().unwrap(),
            kernel_dst.to_str().unwrap(),
        ])
        .await?;
        sudo_checked(&[
            "cp",
            config.rootfs_path.to_str().unwrap(),
            rootfs_dst.to_str().unwrap(),
        ])
        .await?;
        let owner = format!("{}:{}", config.jail_uid, config.jail_gid);
        sudo_checked(&[
            "chown",
            &owner,
            kernel_dst.to_str().unwrap(),
            rootfs_dst.to_str().unwrap(),
        ])
        .await?;
        debug!("Jail files copied in {:?}", start.elapsed());

        // 5. Configure VM via API (paths are relative to jail root)
        let api = FirecrackerApi::new(&api_socket_path);

        api.set_boot_source(Path::new("vmlinux.bin"), &config.boot_args)
            .await?;

        api.set_rootfs(Path::new("rootfs.ext4")).await?;

        api.set_machine_config(config.vcpu_count, config.mem_size_mib)
            .await?;

        let mac = format!("AA:FC:00:00:00:{:02X}", vm_id.as_bytes()[0]);
        api.set_network_interface("eth0", &namespace.tap_name, &mac)
            .await?;

        // Vsock path is relative to jail root
        api.set_vsock(3, vsock_name).await?;
        debug!("VM configured in {:?}", start.elapsed());

        // 6. Start the VM
        api.instance_start().await?;

        // 7. Wait for vsock UDS to appear and fix permissions
        wait_for_socket(&vsock_uds_path, Duration::from_secs(10)).await?;
        debug!("VM {vm_id} Firecracker started in {:?}", start.elapsed());

        // 8. Wait for guest vsock worker to be ready (~7-10s for Alpine boot)
        let mut guest_ready = false;
        for attempt in 1..=30 {
            match crate::vsock::transport::VsockConnection::connect_vsock(&vsock_uds_path, 5000)
                .await
            {
                Ok(_conn) => {
                    guest_ready = true;
                    break;
                }
                Err(e) => {
                    debug!("VM {vm_id} vsock not ready (attempt {attempt}/30): {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
        if !guest_ready {
            warn!("VM {vm_id} guest worker not ready after 30s, shutting down");
            // Can't call self.shutdown() since we haven't constructed Self yet.
            // Kill process and clean up manually.
            let _ = process.kill().await;
            namespace.cleanup().await;
            return Err(VmError::Timeout(
                "Guest vsock worker not ready after 30s".to_string(),
            ));
        }
        info!("VM {vm_id} ready in {:?}", start.elapsed());

        let host_ip = namespace.veth_host_ip.clone();
        Ok(Self {
            vm_id,
            namespace,
            api_socket_path,
            vsock_uds_path,
            host_ip,
            jail_root,
            process,
            start_time: start,
        })
    }

    /// Restore a VM from a snapshot using the jailer.
    pub async fn restore(snapshot_dir: &Path, config: &VmConfig) -> Result<Self, VmError> {
        let vm_id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>();
        let start = Instant::now();

        info!("Restoring VM {vm_id} from snapshot via jailer");

        // 1. Create namespace
        let namespace = VmNamespace::create().await?;

        // 2. Jail root (created by jailer)
        let jail_root = PathBuf::from(format!("/srv/jailer/firecracker/{vm_id}/root"));

        // 3. Start jailer first (creates the chroot directory)
        let api_sock_name = "firecracker.socket";
        let api_socket_path = jail_root.join(api_sock_name).to_str().unwrap().to_string();
        let vsock_name = "vsock.sock";
        let vsock_uds_path = jail_root.join(vsock_name).to_str().unwrap().to_string();

        let process = start_jailer(config, &vm_id, &namespace.name, api_sock_name).await?;
        wait_for_socket(&api_socket_path, Duration::from_secs(10)).await?;

        // 4. Copy snapshot files into the jail (fail early if copies fail)
        for filename in ["vmstate", "memory", "rootfs.ext4"] {
            sudo_checked(&[
                "cp",
                snapshot_dir.join(filename).to_str().unwrap(),
                jail_root.join(filename).to_str().unwrap(),
            ])
            .await?;
        }
        let owner = format!("{}:{}", config.jail_uid, config.jail_gid);
        sudo_checked(&["chown", "-R", &owner, jail_root.to_str().unwrap()]).await?;

        // 5. Load snapshot (paths relative to jail root)
        let api = FirecrackerApi::new(&api_socket_path);
        api.load_snapshot("vmstate", "memory").await?;
        debug!("Snapshot loaded in {:?}", start.elapsed());

        // 5. Resume
        api.set_vm_state("Resumed").await?;

        // 6. Wait for vsock UDS
        wait_for_socket(&vsock_uds_path, Duration::from_secs(10)).await?;
        info!("VM {vm_id} restored in {:?}", start.elapsed());

        let host_ip = namespace.veth_host_ip.clone();
        Ok(Self {
            vm_id,
            namespace,
            api_socket_path,
            vsock_uds_path,
            host_ip,
            jail_root,
            process,
            start_time: start,
        })
    }

    #[allow(dead_code)]
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Shut down the VM and clean up all resources.
    pub async fn shutdown(mut self) {
        info!("Shutting down VM {}", self.vm_id);

        // Kill the jailer/firecracker process (runs as root via sudo)
        if let Err(e) = self.process.kill().await {
            warn!("Failed to kill jailer process: {e}");
        }
        let _ = self.process.wait().await;

        // Also kill any Firecracker process inside the jail
        sudo(&["pkill", "-f", &format!("--id={}", self.vm_id)]).await;

        // Clean up namespace
        self.namespace.cleanup().await;

        // Remove the entire jail directory tree
        if let Some(jail_parent) = self.jail_root.parent() {
            sudo(&["rm", "-rf", jail_parent.to_str().unwrap()]).await;
        }

        debug!("VM {} cleaned up", self.vm_id);
    }
}

/// Start Firecracker via the jailer.
async fn start_jailer(
    config: &VmConfig,
    vm_id: &str,
    netns: &str,
    api_sock: &str,
) -> Result<Child, VmError> {
    let netns_path = format!("/var/run/netns/{netns}");

    let uid = config.jail_uid.to_string();
    let gid = config.jail_gid.to_string();

    debug!(
        "Starting jailer: id={vm_id} exec={} uid={uid} gid={gid} netns={netns_path} api_sock={api_sock}",
        config.firecracker_path.display()
    );

    let child = Command::new("sudo")
        .args([
            config.jailer_path.to_str().unwrap(),
            "--id",
            vm_id,
            "--exec-file",
            config.firecracker_path.to_str().unwrap(),
            "--uid",
            &uid,
            "--gid",
            &gid,
            "--netns",
            &netns_path,
            "--",
            "--api-sock",
            api_sock,
        ])
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| VmError::Process(format!("Failed to start jailer: {e}")))?;

    Ok(child)
}

/// Wait for a socket file to appear and fix permissions.
/// Uses `sudo test -e` because the jail directory may be owned by root.
async fn wait_for_socket(path: &str, timeout: Duration) -> Result<(), VmError> {
    let deadline = Instant::now() + timeout;
    loop {
        let exists = Command::new("sudo")
            .args(["test", "-e", path])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if exists {
            // Make the socket and its parent directories accessible
            let _ = Command::new("sudo")
                .args(["chmod", "755", "/srv/jailer", "/srv/jailer/firecracker"])
                .output()
                .await;
            // Make the jail directory tree traversable and the socket accessible
            let path_buf = std::path::PathBuf::from(path);
            if let Some(root_dir) = path_buf.parent() {
                sudo(&["chmod", "-R", "755", root_dir.to_str().unwrap()]).await;
                if let Some(id_dir) = root_dir.parent() {
                    sudo(&["chmod", "755", id_dir.to_str().unwrap()]).await;
                }
            }
            sudo(&["chmod", "666", path]).await;
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(VmError::Timeout(format!(
                "Socket not found at {path} after {timeout:?}"
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
