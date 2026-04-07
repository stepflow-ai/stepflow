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

//! Generated protobuf types and gRPC service definitions for the Stepflow v1 protocol.
//!
//! This crate compiles all `.proto` files and exports the generated Rust types,
//! gRPC service traits, and client stubs. Service *implementations* live in
//! `stepflow-grpc`.

/// Generated protobuf types for the Stepflow v1 protocol.
pub mod stepflow {
    pub mod v1 {
        tonic::include_proto!("stepflow.v1");
    }
}

pub mod google {
    pub mod api {
        tonic::include_proto!("google.api");
    }
}

// Re-export commonly used types at the crate root.
pub use stepflow::v1::*;

#[cfg(feature = "tokio")]
pub mod async_io;
