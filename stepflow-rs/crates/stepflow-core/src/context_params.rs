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

use crate::BlobId;
use crate::workflow::{Flow, ValueRef, WorkflowOverrides};
use std::{collections::HashMap, sync::Arc};

/// Parameters for submitting a workflow for execution
#[derive(Debug, Clone)]
pub struct SubmitFlowParams {
    pub flow: Arc<Flow>,
    pub flow_id: BlobId,
    pub input: ValueRef,
    pub parent_context: Option<stepflow_observability::fastrace::prelude::SpanContext>,
    pub overrides: Option<WorkflowOverrides>,
    pub variables: Option<HashMap<String, ValueRef>>,
}

impl SubmitFlowParams {
    pub fn new(flow: Arc<Flow>, flow_id: BlobId, input: ValueRef) -> Self {
        Self {
            flow,
            flow_id,
            input,
            parent_context: None,
            overrides: None,
            variables: None,
        }
    }

    pub fn with_parent_context(
        mut self,
        parent_context: stepflow_observability::fastrace::prelude::SpanContext,
    ) -> Self {
        self.parent_context = Some(parent_context);
        self
    }

    pub fn with_overrides(mut self, overrides: WorkflowOverrides) -> Self {
        self.overrides = Some(overrides);
        self
    }

    pub fn with_variables(mut self, variables: HashMap<String, ValueRef>) -> Self {
        self.variables = Some(variables);
        self
    }
}

/// Parameters for submitting a batch workflow execution
#[derive(Debug, Clone)]
pub struct SubmitBatchParams {
    pub flow: Arc<Flow>,
    pub flow_id: BlobId,
    pub inputs: Vec<ValueRef>,
    pub max_concurrency: Option<usize>,
    pub parent_context: Option<stepflow_observability::fastrace::prelude::SpanContext>,
    pub overrides: Option<WorkflowOverrides>,
    pub variables: Option<HashMap<String, ValueRef>>,
}

impl SubmitBatchParams {
    pub fn new(flow: Arc<Flow>, flow_id: BlobId, inputs: Vec<ValueRef>) -> Self {
        Self {
            flow,
            flow_id,
            inputs,
            max_concurrency: None,
            parent_context: None,
            overrides: None,
            variables: None,
        }
    }

    pub fn with_max_concurrency(mut self, max_concurrency: usize) -> Self {
        self.max_concurrency = Some(max_concurrency);
        self
    }

    pub fn with_parent_context(
        mut self,
        parent_context: stepflow_observability::fastrace::prelude::SpanContext,
    ) -> Self {
        self.parent_context = Some(parent_context);
        self
    }

    pub fn with_overrides(mut self, overrides: WorkflowOverrides) -> Self {
        self.overrides = Some(overrides);
        self
    }

    pub fn with_variables(mut self, variables: HashMap<String, ValueRef>) -> Self {
        self.variables = Some(variables);
        self
    }
}
