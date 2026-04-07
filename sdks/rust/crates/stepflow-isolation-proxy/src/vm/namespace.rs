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

//! Network namespace and TAP device management for Firecracker VMs.
//!
//! Each VM runs in its own network namespace with:
//! - A veth pair connecting the namespace to the host
//! - A TAP device (vmtap0) inside the namespace for the VM's eth0
//! - NAT rules for outbound connectivity (VM → orchestrator)
//!
//! Guest subnet: 192.168.241.0/29
//!   - TAP (gateway): 192.168.241.1
//!   - VM:            192.168.241.2

use std::sync::atomic::{AtomicU32, Ordering};

use log::{debug, info};
use tokio::process::Command;

/// Guest subnet configuration (same for all VMs, NAT provides unique external addressing).
/// The TAP gateway IP is how VMs reach the host (used for URL rewriting in dispatch).
pub const GUEST_TAP_IP: &str = "192.168.241.1";
#[allow(dead_code)]
const GUEST_TAP_NETMASK: &str = "255.255.255.248";

/// Counters for unique naming — start from PID to avoid collisions with
/// leftover resources from previous proxy runs.
static NAMESPACE_COUNTER: AtomicU32 = AtomicU32::new(0);
static VETH_IP_COUNTER: AtomicU32 = AtomicU32::new(0);
static COUNTERS_INITIALIZED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn ensure_counters_initialized() {
    if !COUNTERS_INITIALIZED.swap(true, Ordering::Relaxed) {
        // Use PID * 10 as base to avoid collisions across runs.
        // Each run gets a unique range (e.g., PID 1234 → ns-12340, veth12340, IP 10.0.0.42)
        let base = (std::process::id() % 1000) * 10;
        NAMESPACE_COUNTER.store(base, Ordering::Relaxed);
        // IP octet: 2-254 range, derived from PID
        VETH_IP_COUNTER.store((base % 250) + 2, Ordering::Relaxed);
    }
}

/// A network namespace with veth pair and TAP device for a Firecracker VM.
pub struct VmNamespace {
    pub name: String,
    pub veth_host: String,
    /// Host-side veth IP — the IP VMs use to reach host services.
    pub veth_host_ip: String,
    #[allow(dead_code)]
    pub veth_ns: String,
    #[allow(dead_code)]
    pub veth_ns_ip: String,
    pub tap_name: String,
}

impl VmNamespace {
    /// Create a new network namespace with veth pair and TAP device.
    pub async fn create() -> Result<Self, NamespaceError> {
        ensure_counters_initialized();
        let id = NAMESPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let veth_ip_octet = VETH_IP_COUNTER.fetch_add(1, Ordering::Relaxed);

        let name = format!("fc-ns-{id}");
        let veth_host = format!("veth{id}");
        let veth_ns = "veth0".to_string();
        // Each namespace gets a unique /30 subnet to avoid routing conflicts.
        // Subnet N uses 10.N.0.0/30: host=10.N.0.1, ns=10.N.0.2
        let subnet_id = (veth_ip_octet % 254) + 1; // Keep second octet in valid range 1-254
        let veth_host_ip = format!("10.{subnet_id}.0.1");
        let veth_ns_ip = format!("10.{subnet_id}.0.2");
        let tap_name = "vmtap0".to_string();

        info!("Creating namespace {name} with veth {veth_host} <-> {veth_ns}");

        // Create namespace
        run_cmd("ip", &["netns", "add", &name]).await?;

        // Create veth pair
        run_cmd(
            "ip",
            &[
                "link", "add", &veth_host, "type", "veth", "peer", "name", &veth_ns,
            ],
        )
        .await?;

        // Move NS side into namespace
        run_cmd("ip", &["link", "set", &veth_ns, "netns", &name]).await?;

        // Configure host side with unique /30 subnet
        run_cmd(
            "ip",
            &[
                "addr",
                "add",
                &format!("{veth_host_ip}/30"),
                "dev",
                &veth_host,
            ],
        )
        .await?;
        run_cmd("ip", &["link", "set", &veth_host, "up"]).await?;

        // Configure NS side
        run_ns(
            &name,
            "ip",
            &["addr", "add", &format!("{veth_ns_ip}/30"), "dev", &veth_ns],
        )
        .await?;
        run_ns(&name, "ip", &["link", "set", &veth_ns, "up"]).await?;
        run_ns(&name, "ip", &["link", "set", "lo", "up"]).await?;

        // Create TAP device inside namespace with a fixed MAC address.
        // Using a consistent MAC across all namespaces ensures the guest's ARP cache
        // (captured in the snapshot) remains valid after restore into a new namespace.
        run_ns(&name, "ip", &["tuntap", "add", &tap_name, "mode", "tap"]).await?;
        run_ns(
            &name,
            "ip",
            &["link", "set", &tap_name, "address", "02:FC:00:00:00:01"],
        )
        .await?;
        run_ns(
            &name,
            "ip",
            &[
                "addr",
                "add",
                &format!("{GUEST_TAP_IP}/29"),
                "dev",
                &tap_name,
            ],
        )
        .await?;
        run_ns(&name, "ip", &["link", "set", &tap_name, "up"]).await?;

        // Set up routing and NAT inside namespace
        run_ns(&name, "sysctl", &["-w", "net.ipv4.ip_forward=1"]).await?;
        run_ns(
            &name,
            "ip",
            &["route", "add", "default", "via", &veth_host_ip],
        )
        .await?;

        // MASQUERADE: guest traffic going out through veth0
        run_ns(
            &name,
            "iptables",
            &[
                "-t",
                "nat",
                "-A",
                "POSTROUTING",
                "-s",
                "192.168.241.0/29",
                "-o",
                &veth_ns,
                "-j",
                "MASQUERADE",
            ],
        )
        .await?;

        Ok(Self {
            name,
            veth_host,
            veth_host_ip,
            veth_ns,
            veth_ns_ip,
            tap_name,
        })
    }

    /// Clean up the namespace and associated resources.
    pub async fn cleanup(&self) {
        debug!("Cleaning up namespace {}", self.name);
        let _ = run_cmd("ip", &["netns", "del", &self.name]).await;
        let _ = run_cmd("ip", &["link", "del", &self.veth_host]).await;
    }
}

/// Error during namespace operations.
#[derive(Debug, thiserror::Error)]
pub enum NamespaceError {
    #[error("Command failed: {cmd} — {detail}")]
    CommandFailed { cmd: String, detail: String },
}

/// Run a command with `sudo`.
async fn run_cmd(program: &str, args: &[&str]) -> Result<(), NamespaceError> {
    debug!("Running: sudo {program} {}", args.join(" "));
    let output = Command::new("sudo")
        .arg(program)
        .args(args)
        .output()
        .await
        .map_err(|e| NamespaceError::CommandFailed {
            cmd: format!("sudo {program} {}", args.join(" ")),
            detail: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NamespaceError::CommandFailed {
            cmd: format!("sudo {program} {}", args.join(" ")),
            detail: stderr.to_string(),
        });
    }
    Ok(())
}

/// Run a command inside a network namespace with `sudo ip netns exec`.
async fn run_ns(ns: &str, program: &str, args: &[&str]) -> Result<(), NamespaceError> {
    let mut full_args = vec!["ip", "netns", "exec", ns, program];
    full_args.extend_from_slice(args);
    run_cmd(full_args[0], &full_args[1..]).await
}
