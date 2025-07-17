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

use std::path::Path;
use std::{pin::Pin, sync::Arc};
use stepflow_core::workflow::FlowHash;
use stepflow_core::{
    FlowResult,
    workflow::{Flow, ValueRef},
};
use stepflow_plugin::ExecutionContext;
use stepflow_state::InMemoryStateStore;
use uuid::Uuid;

/// A mock execution context for testing built-in components.
///
/// This provides a way to create ExecutionContext instances for testing
/// built-in components without requiring complex workflow execution.
pub struct MockContext {
    executor: Arc<MockExecutor>,
}

impl MockContext {
    /// Create a new mock context.
    pub fn new() -> Self {
        Self {
            executor: Arc::new(MockExecutor {
                state_store: Arc::new(InMemoryStateStore::new()),
            }),
        }
    }

    /// Get an execution context for testing from this mock context.
    pub fn execution_context(&self) -> ExecutionContext {
        ExecutionContext::new(self.executor.clone(), Uuid::new_v4())
    }
}

impl Default for MockContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock executor implementation for testing.
struct MockExecutor {
    state_store: Arc<dyn stepflow_state::StateStore>,
}

impl stepflow_plugin::Context for MockExecutor {
    fn submit_flow(
        &self,
        _flow: Arc<Flow>,
        _workflow_hash: FlowHash,
        _input: ValueRef,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<Uuid>> + Send + '_>> {
        Box::pin(async { Ok(Uuid::new_v4()) })
    }

    fn flow_result(
        &self,
        _run_id: Uuid,
    ) -> Pin<Box<dyn std::future::Future<Output = stepflow_plugin::Result<FlowResult>> + Send + '_>>
    {
        Box::pin(async {
            let result = serde_json::json!({"message": "Hello from nested flow"});
            Ok(FlowResult::Success(ValueRef::new(result)))
        })
    }

    fn state_store(&self) -> &Arc<dyn stepflow_state::StateStore> {
        &self.state_store
    }

    fn working_directory(&self) -> &std::path::Path {
        Path::new(".")
    }
}
