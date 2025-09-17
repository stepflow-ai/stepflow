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

use std::{collections::HashMap, sync::Arc};

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::{
    FlowResult,
    component::ComponentInfo,
    schema::SchemaRef,
    workflow::{Component, ValueRef},
};
use stepflow_plugin::{
    Context, DynPlugin, ExecutionContext, Plugin, PluginConfig, PluginError, Result,
};

impl PluginConfig for MockPlugin {
    type Error = PluginError;

    async fn create_plugin(
        self,
        _working_directory: &std::path::Path,
    ) -> error_stack::Result<Box<DynPlugin<'static>>, Self::Error> {
        Ok(DynPlugin::boxed(self))
    }
}

/// A mock plugin that can be used to test various things in the plugin protocol.
#[derive(Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct MockPlugin {
    components: HashMap<Component, MockComponent>,
}

/// Enumeration of behaviors for the mock components.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged, rename_all = "camelCase")]
pub enum MockComponentBehavior {
    /// Produce the given internal (non-flow) error.
    Error { error: String },
    /// Return the given result (success or flow-error).
    Result {
        #[serde(flatten)]
        result: FlowResult,
    },
}

impl MockComponentBehavior {
    pub fn result(result: impl Into<FlowResult>) -> Self {
        let result = result.into();
        Self::Result { result }
    }

    pub fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::Error { error: message }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct MockComponent {
    input_schema: SchemaRef,
    output_schema: SchemaRef,
    behaviors: HashMap<ValueRef, MockComponentBehavior>,
}

impl Default for MockComponent {
    fn default() -> Self {
        Self {
            input_schema: SchemaRef::parse_json(r#"{"type": "object"}"#).unwrap(),
            output_schema: SchemaRef::parse_json(r#"{"type": "object"}"#).unwrap(),
            behaviors: HashMap::new(),
        }
    }
}

impl MockComponent {
    pub fn input_schema(&mut self, input_schema: SchemaRef) -> &mut Self {
        self.input_schema = input_schema;
        self
    }

    pub fn output_schema(&mut self, output_schema: SchemaRef) -> &mut Self {
        self.output_schema = output_schema;
        self
    }

    pub fn behavior(
        &mut self,
        input: impl Into<ValueRef>,
        behavior: MockComponentBehavior,
    ) -> &mut Self {
        self.behaviors.insert(input.into(), behavior);
        self
    }
}

impl MockPlugin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mock_component(&mut self, path: &str) -> &mut MockComponent {
        let component = Component::from_string(path);
        self.components.entry(component).or_default()
    }
}

impl Plugin for MockPlugin {
    async fn init(&self, _context: &Arc<dyn Context>) -> Result<()> {
        Ok(())
    }

    async fn list_components(&self) -> Result<Vec<ComponentInfo>> {
        Ok(self
            .components
            .iter()
            .map(|(component, mock_component)| ComponentInfo {
                component: component.clone(),
                input_schema: Some(mock_component.input_schema.clone()),
                output_schema: Some(mock_component.output_schema.clone()),
                description: None,
            })
            .collect())
    }

    async fn component_info(&self, component: &Component) -> Result<ComponentInfo> {
        let (component, mock_component) = self
            .components
            .get_key_value(component)
            .ok_or(PluginError::UdfImport)?;
        Ok(ComponentInfo {
            component: component.clone(),
            input_schema: Some(mock_component.input_schema.clone()),
            output_schema: Some(mock_component.output_schema.clone()),
            description: None,
        })
    }

    async fn execute(
        &self,
        component: &Component,
        _context: ExecutionContext,
        input: ValueRef,
    ) -> Result<FlowResult> {
        let mock_component = self
            .components
            .get(component)
            .ok_or(PluginError::UdfImport)
            .attach_printable_lazy(|| component.clone())?;
        // Debug logging for tests only - not included in production builds
        #[cfg(test)]
        tracing::debug!("Mock plugin executing component: {}", component);

        let input_value = input.value();
        let output = mock_component
            .behaviors
            .get(&input)
            .ok_or(PluginError::UdfExecution)
            .attach_printable_lazy(|| {
                format!("No behavior defined for {component} on {input_value}")
            })?;

        match output {
            MockComponentBehavior::Error { .. } => error_stack::bail!(PluginError::UdfExecution),
            MockComponentBehavior::Result { result } => Ok(result.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static_assertions::assert_impl_all!(MockPlugin: Send, Sync);
}
