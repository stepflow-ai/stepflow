// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use futures::future::{BoxFuture, FutureExt as _};
use std::path::Path;
use std::sync::Arc;
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_state::StateStore;
use uuid::Uuid;

/// Trait for interacting with the workflow runtime.
pub trait Context: Send + Sync {
    /// Submits a nested workflow for execution and returns its execution ID.
    ///
    /// Implementation should use Arc::clone on self if it needs to pass ownership
    /// to spawned tasks or other async contexts.
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<Uuid>>;

    /// Retrieves the result of a previously submitted workflow.
    fn flow_result(&self, run_id: Uuid) -> BoxFuture<'_, crate::Result<FlowResult>>;

    /// Executes a nested workflow and waits for its completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        async move {
            let run_id = self.submit_flow(flow, workflow_hash, input).await?;
            self.flow_result(run_id).await
        }
        .boxed()
    }

    /// Get the state store for this executor.
    fn state_store(&self) -> &Arc<dyn StateStore>;

    /// Working directory of the StepFlow Config.
    fn working_directory(&self) -> &Path;
}

/// Execution context that combines a Context with an execution ID.
#[derive(Clone)]
pub struct ExecutionContext {
    context: Arc<dyn Context>,
    run_id: Uuid,
}

impl ExecutionContext {
    /// Create a new ExecutionContext.
    pub fn new(context: Arc<dyn Context>, run_id: Uuid) -> Self {
        Self { context, run_id }
    }

    /// Get the execution ID for this context.
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Get a reference to the state store.
    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }
}

impl Context for ExecutionContext {
    fn state_store(&self) -> &Arc<dyn StateStore> {
        self.context.state_store()
    }

    /// Submit a nested workflow for execution.
    fn submit_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<Uuid>> {
        self.context.submit_flow(flow, workflow_hash, input)
    }

    /// Get the result of a workflow execution.
    fn flow_result(&self, run_id: Uuid) -> BoxFuture<'_, crate::Result<FlowResult>> {
        self.context.flow_result(run_id)
    }

    /// Execute a nested workflow and wait for completion.
    fn execute_flow(
        &self,
        flow: Arc<Flow>,
        workflow_hash: FlowHash,
        input: ValueRef,
    ) -> BoxFuture<'_, crate::Result<FlowResult>> {
        self.context.execute_flow(flow, workflow_hash, input)
    }

    fn working_directory(&self) -> &Path {
        self.context.working_directory()
    }
}
