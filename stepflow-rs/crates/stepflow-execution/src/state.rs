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

//! Execution state tracking.
//!
//! This module provides state tracking for workflow execution:
//! - [`ItemState`] - tracks execution state for a single item
//! - [`ItemsState`] - manages multiple items for batch execution
//! - [`StepIndex`] - efficient step ID to index mapping

mod item_state;
mod items_state;

pub use item_state::{ItemState, StepIndex};
pub use items_state::ItemsState;
