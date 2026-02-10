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

//! Stepflow HTTP server library
//!
//! This crate provides the core HTTP server functionality for Stepflow.
//! It contains all the API endpoints, request/response types, and server startup logic.

mod api;
pub mod error;
mod orphan_recovery;
mod shutdown;
mod startup;

// Explicit exports from api module
pub use api::{CreateRunRequest, CreateRunResponse, StoreFlowRequest, StoreFlowResponse};

// Startup configuration
pub use startup::{AppConfig, create_environment, start_server};

// Server lifecycle
pub use orphan_recovery::orphan_claiming_loop;
pub use shutdown::shutdown_signal;
