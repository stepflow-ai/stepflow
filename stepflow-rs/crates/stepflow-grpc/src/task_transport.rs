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

//! Transport abstraction for dispatching tasks to workers.
//!
//! The [`TaskTransport`] trait defines how the orchestrator sends tasks to
//! workers. Different implementations support different backends:
//!
//! - **In-process / SQLite**: Tasks are stored in a shared table and served
//!   by `TasksService::PullTasks` to connected workers.
//! - **NATS**: Tasks are published to a NATS JetStream subject.
//! - **Kafka**: Tasks are published to a Kafka topic.
//!
//! Workers always report task completion via
//! `OrchestratorService::CompleteTask`, regardless of transport.

use stepflow_core::component::ComponentInfo;
use stepflow_plugin::{PluginError, Result};

use crate::proto::stepflow::v1::TaskAssignment;

/// Transport for dispatching tasks from the orchestrator to workers.
///
/// Implementations are responsible for delivering [`TaskAssignment`] messages
/// to workers via the appropriate backend. The orchestrator plugin
/// (`StepflowQueuePlugin`) calls `send_task` for each component execution
/// and waits for the result via the `TaskCompletionRegistry`.
#[tonic::async_trait]
pub trait TaskTransport: Send + Sync + 'static {
    /// Dispatch a task to the worker queue.
    ///
    /// The transport should deliver the task to workers matching the
    /// component path. Returns once the task is durably enqueued (not
    /// necessarily delivered to a worker).
    async fn send_task(&self, task: TaskAssignment) -> Result<()>;

    /// List components available from workers connected via this transport.
    ///
    /// For pull-based transports, this returns components from currently
    /// connected workers. For queue-based transports, this may return a
    /// cached or configured set of components.
    async fn list_components(&self) -> Result<Vec<ComponentInfo>>;

    /// Get component info for a specific component by name.
    ///
    /// Default implementation searches `list_components()`. Transports
    /// with more efficient lookup can override this.
    async fn component_info(&self, component: &str) -> Result<ComponentInfo> {
        let components = self.list_components().await?;
        components
            .into_iter()
            .find(|c| c.component.path() == component)
            .ok_or_else(|| {
                error_stack::report!(PluginError::ComponentInfo)
                    .attach_printable(format!("component '{component}' not found"))
            })
    }
}
