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

use std::{path::Path, sync::Arc};

use crate::{ExecutionContext, Result, StepflowEnvironment};
use serde::{Serialize, de::DeserializeOwned};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};

#[trait_variant::make(Send)]
#[dynosaur::dynosaur(pub DynPlugin = dyn Plugin)]
pub trait Plugin: Send + Sync {
    /// Ensure the plugin is initialized with the shared environment.
    ///
    /// This method is idempotent - calling it multiple times has no additional effect
    /// after the first successful initialization. Plugins should track their own
    /// initialization state and return `Ok(())` immediately if already initialized.
    ///
    /// Called during environment setup, before any runs are executed.
    /// Plugins can use this to connect to external services, start background tasks, etc.
    async fn ensure_initialized(&self, env: &Arc<StepflowEnvironment>) -> Result<()>;

    /// List all components available in this plugin.
    async fn list_components(&self) -> Result<Vec<ComponentInfo>>;

    /// Return the outputs for the given component.
    async fn component_info(&self, component: &Component) -> Result<ComponentInfo>;

    /// Execute the step and return the resulting arguments.
    ///
    /// The arguments should be fully resolved.
    async fn execute(
        &self,
        component: &Component,
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult>;
}

/// Trait implemented by a deserializable plugin configuration.
pub trait PluginConfig: Serialize + DeserializeOwned {
    type Error: error_stack::Context;

    fn create_plugin(
        self,
        working_directory: &Path,
    ) -> impl Future<Output = error_stack::Result<Box<DynPlugin<'static>>, Self::Error>>;
}
