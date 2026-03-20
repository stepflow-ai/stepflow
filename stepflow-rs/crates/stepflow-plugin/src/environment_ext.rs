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

//! Extension trait for PluginRouter access in StepflowEnvironment.

use std::sync::Arc;

use error_stack::ResultExt as _;
use stepflow_core::StepflowEnvironment;
use stepflow_core::workflow::{Component, ValueRef};

use crate::routing::PluginRouter;
use crate::{DynPlugin, PluginError, Result};

/// Extension trait providing PluginRouter access for StepflowEnvironment.
///
/// This trait allows crates that need plugin routing access to import this
/// extension and call `env.plugin_router()` without requiring stepflow-core
/// to have any knowledge of the PluginRouter type.
///
/// # Example
///
/// ```ignore
/// use stepflow_plugin::PluginRouterExt;
///
/// async fn get_plugin(env: &StepflowEnvironment, component: &Component, input: ValueRef) {
///     let (plugin, resolved_name, route_params) = env.get_plugin_and_component(component, input).unwrap();
/// }
/// ```
pub trait PluginRouterExt {
    /// Get the plugin router.
    ///
    /// # Panics
    ///
    /// Panics if plugin router was not set during environment construction.
    fn plugin_router(&self) -> Arc<PluginRouter>;

    /// Get a plugin, resolved component name, and route params for execution.
    fn get_plugin_and_component(
        &self,
        component: &Component,
        input: ValueRef,
    ) -> Result<(
        Arc<DynPlugin<'static>>,
        String,
        std::collections::HashMap<String, serde_json::Value>,
    )>;

    /// List all registered plugins.
    fn plugins(&self) -> Vec<Arc<DynPlugin<'static>>>;
}

impl PluginRouterExt for StepflowEnvironment {
    fn plugin_router(&self) -> Arc<PluginRouter> {
        self.get::<Arc<PluginRouter>>()
            .expect("PluginRouter not set in environment")
    }

    fn get_plugin_and_component(
        &self,
        component: &Component,
        input: ValueRef,
    ) -> Result<(
        Arc<DynPlugin<'static>>,
        String,
        std::collections::HashMap<String, serde_json::Value>,
    )> {
        let router = self.plugin_router();
        let (plugin, name, route_params) = router
            .get_plugin_and_component(component.path(), input)
            .change_context(PluginError::InvalidInput)?;
        Ok((plugin.clone(), name, route_params))
    }

    fn plugins(&self) -> Vec<Arc<DynPlugin<'static>>> {
        self.plugin_router().plugins().cloned().collect()
    }
}
