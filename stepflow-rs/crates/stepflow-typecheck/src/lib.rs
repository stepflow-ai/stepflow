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

//! Type checking and inference for Stepflow workflows.
//!
//! This crate provides static type checking for Stepflow workflows, catching type
//! mismatches before execution. It uses forward propagation to synthesize types
//! through the workflow DAG.
//!
//! # Key Concepts
//!
//! - **Type**: Represents either a concrete JSON Schema, `Any` (matches everything),
//!   or `Never` (impossible type).
//! - **TypeEnvironment**: Tracks types for workflow inputs, variables, and step outputs.
//! - **Subtyping**: Structural subtyping based on JSON Schema semantics.
//! - **Synthesis**: Infers types from value expressions.

mod checker;
mod environment;
mod error;
mod input_inference;
mod subtyping;
mod synthesis;
mod types;

pub use checker::{ComponentSchemaProvider, TypeCheckConfig, TypeCheckResult, type_check_flow};
pub use environment::TypeEnvironment;
pub use error::{LocatedTypeError, TypeError, TypeErrorLocation};
pub use input_inference::{
    InputConstraint, InputConstraintConflict, build_input_schema, collect_input_constraints,
    find_constraint_conflicts,
};
pub use subtyping::{SubtypeResult, is_subtype};
pub use synthesis::{SynthesisResult, infer_type_from_value, synthesize_type, union_types};
pub use types::Type;
