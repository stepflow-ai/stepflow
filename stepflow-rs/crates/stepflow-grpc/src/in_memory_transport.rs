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

//! In-memory task transport for pull-based worker dispatch.
//!
//! [`InMemoryTaskTransport`] implements [`TaskTransport`] by writing tasks
//! to a shared [`PullTaskQueue`]. Workers consume tasks via
//! `TasksService::PullTasks`, which reads from the same queue.

use std::sync::Arc;

use stepflow_core::component::ComponentInfo;
use stepflow_plugin::Result;

use crate::proto::stepflow::v1::TaskAssignment;
use crate::pull_task_queue::PullTaskQueue;
use crate::task_transport::TaskTransport;

/// In-memory transport backed by a [`PullTaskQueue`].
///
/// Tasks are held in memory and served to workers via `TasksService.PullTasks`.
/// This is the default transport for local development and single-node
/// deployments.
pub struct InMemoryTaskTransport {
    queue: Arc<PullTaskQueue>,
}

impl InMemoryTaskTransport {
    /// Create a new in-memory transport backed by the given queue.
    ///
    /// The same `PullTaskQueue` must be shared with `TasksServiceImpl`.
    pub fn new(queue: Arc<PullTaskQueue>) -> Self {
        Self { queue }
    }

    /// Get a reference to the shared queue.
    pub fn queue(&self) -> &Arc<PullTaskQueue> {
        &self.queue
    }
}

#[tonic::async_trait]
impl TaskTransport for InMemoryTaskTransport {
    async fn send_task(&self, task: TaskAssignment) -> Result<()> {
        self.queue.push_task(task);
        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        Ok(self.queue.list_components())
    }
}
