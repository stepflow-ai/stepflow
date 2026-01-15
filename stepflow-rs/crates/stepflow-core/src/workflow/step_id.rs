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

use std::sync::Arc;

use crate::workflow::Flow;

/// A step identifier that combines the step index with a reference to the flow.
/// This allows efficient lookup using the index while maintaining access to
/// the step ID string and other step metadata through the flow reference.
///
/// NOTE: Using this will keep the Flow alive.
#[derive(Clone)]
pub struct StepId {
    /// Reference to the flow containing this step
    pub flow: Arc<Flow>,
    /// The step index in the workflow
    pub index: usize,
}

impl StepId {
    pub fn for_step(flow: Arc<Flow>, index: usize) -> Self {
        Self { flow, index }
    }
}

impl PartialEq for StepId {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && Arc::ptr_eq(&self.flow, &other.flow)
    }
}

impl Eq for StepId {}

impl PartialOrd for StepId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if Arc::ptr_eq(&self.flow, &other.flow) {
            Some(self.index.cmp(&other.index))
        } else {
            None
        }
    }
}

impl std::hash::Hash for StepId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        Arc::as_ptr(&self.flow).hash(state);
    }
}

impl StepId {
    /// Get the step name/ID string from the flow
    pub fn step_name(&self) -> &str {
        &self.flow.steps[self.index].id
    }
}

impl std::fmt::Display for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.step_name())
    }
}

impl std::fmt::Debug for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.step_name())
    }
}
