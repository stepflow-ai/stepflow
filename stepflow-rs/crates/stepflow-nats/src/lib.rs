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

//! NATS JetStream task transport for Stepflow.
//!
//! This crate provides a [`NatsTaskTransport`] that publishes task assignments
//! to NATS JetStream subjects, enabling fully decoupled orchestrator-worker
//! communication via a message broker.
//!
//! Workers subscribe directly to NATS subjects — no gRPC `PullTasks` needed
//! for task dispatch. Workers still report heartbeats, completion, and
//! sub-runs via the gRPC `OrchestratorService` (URL from `TaskContext`).

pub mod nats_journal;
pub mod nats_plugin_config;
pub mod nats_transport;

pub use nats_journal::NatsJournal;
pub use nats_plugin_config::{NatsPluginConfig, NatsPluginFactory};
pub use nats_transport::NatsTaskTransport;
