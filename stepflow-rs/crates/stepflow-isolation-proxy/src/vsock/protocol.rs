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

//! Re-export vsock protocol from stepflow-worker.
//!
//! The general-purpose length-delimited protobuf framing lives in
//! `stepflow_worker::vsock`. This module re-exports it for convenience.

pub use stepflow_worker::vsock::write_message;
