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
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

/// Parameters for submitting a run (1 or N items).
#[derive(Debug, Clone)]
pub struct SubmitRunParams {
    pub flow: Arc<Flow>,
    pub flow_id: BlobId,
    pub inputs: Vec<ValueRef>,
    pub wait: bool,
    pub max_concurrency: Option<usize>,
    pub parent_context: Option<stepflow_observability::fastrace::prelude::SpanContext>,
    pub overrides: Option<WorkflowOverrides>,
    pub variables: Option<HashMap<String, ValueRef>>,
}

impl SubmitRunParams {
    /// Create params with the given inputs.
    ///
    /// For single-item runs, pass `vec![input]`.
    pub fn new(flow: Arc<Flow>, flow_id: BlobId, inputs: Vec<ValueRef>) -> Self {
        Self {
            flow,
            flow_id,
            inputs,
            wait: false,
            max_concurrency: None,
            parent_context: None,
            overrides: None,
            variables: None,
        }
    }

    pub fn with_wait(mut self, wait: bool) -> Self {
        self.wait = wait;
        self
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

    /// Number of items in this run.
    pub fn item_count(&self) -> usize {
        self.inputs.len()
    }
}

/// Options for getting run status and results.
#[derive(Debug, Clone, Default)]
pub struct GetRunOptions {
    /// Wait for run completion before returning.
    pub wait: bool,
    /// Include item results in the response.
    pub include_results: bool,
    /// Order of results (only relevant if include_results=true).
    pub result_order: ResultOrder,
}

impl GetRunOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_wait(mut self, wait: bool) -> Self {
        self.wait = wait;
        self
    }

    pub fn with_results(mut self) -> Self {
        self.include_results = true;
        self
    }

    pub fn with_result_order(mut self, order: ResultOrder) -> Self {
        self.result_order = order;
        self
    }
}

/// Order in which to return results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultOrder {
    /// Results in input order (index 0, 1, 2, ...).
    #[default]
    ByIndex,
    /// Results in completion order (first completed first).
    ByCompletion,
}
