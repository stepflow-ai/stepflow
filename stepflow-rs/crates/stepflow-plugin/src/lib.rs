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

mod context;
mod environment_builder;
mod environment_ext;
mod error;
mod plugin;
pub mod routing;
mod subflow;

pub use context::RunContext;
pub use environment_builder::{BlobApiUrl, StepflowEnvironmentBuilder};
pub use environment_ext::PluginRouterExt;
pub use error::{PluginError, Result};
pub use plugin::{DynPlugin, Plugin, PluginConfig};
pub use subflow::{
    SubflowReceiver, SubflowRequest, SubflowSubmitError, SubflowSubmitter, subflow_channel,
};

// Re-export StepflowEnvironment from core for convenience
pub use stepflow_core::StepflowEnvironment;
