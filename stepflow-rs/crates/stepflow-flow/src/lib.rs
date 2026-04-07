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

//! Stepflow workflow definition types.
//!
//! This crate provides the core data types for defining Stepflow workflows:
//! - [`Flow`](workflow::Flow), [`Step`](workflow::Step): Workflow structure
//! - [`ValueExpr`]: Value expressions for data flow between steps
//! - [`FlowResult`], [`FlowError`]: Execution result types
//! - [`SchemaRef`](schema::SchemaRef): JSON Schema references

pub mod discriminator_schema;
pub mod json_schema;
pub mod schema;
pub mod values;
pub mod workflow;

pub mod error_stack;

mod flow_result;
mod task_error_code;

pub use error_stack::{ErrorStack, ErrorStackEntry};
pub use flow_result::*;
pub use task_error_code::TaskErrorCode;
pub use values::ValueExpr;
