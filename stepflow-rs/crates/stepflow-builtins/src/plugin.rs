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

use std::sync::Arc;

use crate::{BuiltinComponent as _, registry};
use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result, StepflowEnvironment,
};

/// The struct that implements the `Plugin` trait.
#[derive(Default)]
pub struct Builtins;

#[derive(Serialize, Deserialize, Debug, utoipa::ToSchema)]
pub struct BuiltinPluginConfig;

impl PluginConfig for BuiltinPluginConfig {
    type Error = PluginError;

    async fn create_plugin(
        self,
        _working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        Ok(DynPlugin::boxed(Builtins::new()))
    }
}

impl Builtins {
    /// Create a new instance with default components registered
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for Builtins {
    async fn ensure_initialized(&self, _env: &Arc<StepflowEnvironment>) -> Result<()> {
        // Builtins require no initialization - always ready
        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        // TODO: Cache?
        registry::components()
            .map(|c| {
                c.component_info()
                    .change_context(PluginError::ComponentInfo)
            })
            .collect()
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let component = registry::get_component(component)?;
        component
            .component_info()
            .change_context(PluginError::ComponentInfo)
    }

    async fn execute(
        &self,
        component: &Component,
        context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let component = registry::get_component(component)?;
        component
            .execute(context, input)
            .await
            .change_context(PluginError::UdfExecution)
    }
}
