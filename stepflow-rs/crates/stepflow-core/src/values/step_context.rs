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

//! Context trait for expression evaluation.
//!
//! The `StepContext` trait provides access to execution state during
//! expression evaluation, including step results, workflow input, and variables.

use crate::FlowResult;
use crate::values::{Secrets, ValueRef};

/// Provides access to execution state for expression evaluation.
///
/// This trait allows `ValueExpr::needed_steps()` to determine which steps
/// are needed to evaluate an expression, and `ValueExpr::resolve()` to
/// evaluate expressions using available state.
pub trait StepContext {
    /// Get the index of a step by its ID.
    ///
    /// Returns `None` if the step ID is not found in the workflow.
    fn step_index(&self, step_id: &str) -> Option<usize>;

    /// Check if a step has completed execution.
    ///
    /// A step is completed if it has a result (success, skipped, or failed).
    fn is_completed(&self, step_index: usize) -> bool;

    /// Get the result of a completed step.
    ///
    /// Returns `None` if the step has not completed or the index is invalid.
    fn get_result(&self, step_index: usize) -> Option<&FlowResult>;

    /// Get the workflow input value.
    ///
    /// Returns `None` if input is not available in this context.
    fn get_input(&self) -> Option<&ValueRef>;

    /// Get a variable value by name.
    ///
    /// Returns `None` if the variable is not defined. The implementation
    /// may fall back to schema defaults before returning `None`.
    /// Returns an owned `ValueRef` since defaults may be computed.
    fn get_variable(&self, name: &str) -> Option<ValueRef>;

    /// Get the secrets configuration for a variable.
    ///
    /// Used for redacting sensitive values in logs.
    fn get_variable_secrets(&self, name: &str) -> Secrets;
}
