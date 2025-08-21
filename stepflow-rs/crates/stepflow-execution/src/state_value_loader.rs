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

use std::sync::Arc;

use async_trait::async_trait;
use stepflow_core::{
    FlowResult,
    values::{ValueLoader, ValueRef},
    workflow::{Flow, StepId},
};
use stepflow_state::StateStore;
use uuid::Uuid;

use crate::{ExecutionError, write_cache::WriteCache};

/// Implementation of ValueLoader that uses StateStore with WriteCache for loading values
#[derive(Clone)]
pub struct StateValueLoader {
    /// Input value for the workflow
    input: ValueRef,
    /// State store to use for resolving values
    state_store: Arc<dyn StateStore>,
    /// Write cache for checking recently completed step results
    write_cache: WriteCache,
    /// The workflow definition for step ID resolution
    flow: Arc<Flow>,
}

impl StateValueLoader {
    pub fn new(
        input: ValueRef,
        state_store: Arc<dyn StateStore>,
        write_cache: WriteCache,
        flow: Arc<Flow>,
    ) -> Self {
        Self {
            input,
            state_store,
            write_cache,
            flow,
        }
    }
}

#[async_trait]
impl ValueLoader for StateValueLoader {
    type Error = ExecutionError;

    async fn load_step_result(
        &self,
        run_id: Uuid,
        step_index: usize,
    ) -> std::result::Result<FlowResult, Self::Error> {
        let step_id = StepId {
            index: step_index,
            flow: self.flow.clone(),
        };

        match self
            .write_cache
            .get_step_result_with_fallback(&step_id, run_id, &self.state_store)
            .await
        {
            Ok(result) => Ok(result),
            Err(_e) => Err(ExecutionError::StateError),
        }
    }

    async fn load_workflow_input(
        &self,
        _run_id: Uuid,
    ) -> std::result::Result<ValueRef, Self::Error> {
        Ok(self.input.clone())
    }
}
