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

//! Value handling and resolution for Stepflow flows.
//!
//! This module contains all value-related functionality:
//! - `ValueExpr`: Expression type for dynamic values and references
//! - `ValueRef`: Arc-wrapped JSON values for efficient storage
//! - `ValueResolver`: Resolution engine for evaluating expressions
//! - `JsonPath`: JSONPath implementation for nested field access
//! - `Secrets`: Secret field redaction for safe logging

pub mod json_path;
pub mod value_expr;
mod value_expr_serde; // Keep private
pub mod redacted_value;
pub mod value_ref;
pub mod value_resolver;

pub use json_path::*;
pub use value_expr::*;
pub use redacted_value::*;
pub use value_ref::*;
pub use value_resolver::*;
