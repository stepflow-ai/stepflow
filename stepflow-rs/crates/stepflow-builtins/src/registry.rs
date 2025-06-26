use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use stepflow_core::workflow::{Component, ComponentKey};
use stepflow_plugin::{PluginError, Result};

use crate::{
    BuiltinComponent, DynBuiltinComponent,
    blob::{GetBlobComponent, PutBlobComponent},
    eval::EvalComponent,
    load_file::LoadFileComponent,
    messages::CreateMessagesComponent,
    openai::OpenAIComponent,
};

#[derive(Default)]
struct Registry {
    components: HashMap<ComponentKey, Arc<DynBuiltinComponent<'static>>>,
}

impl Registry {
    pub fn register<C: BuiltinComponent + 'static>(
        &mut self,
        host: &'static str,
        path: &'static str,
        component: C,
    ) {
        let key = (Some(host.to_string()), path.to_string());
        let component = DynBuiltinComponent::boxed(component);
        let component = Arc::from(component);
        self.components.insert(key, component);
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    let mut registry = Registry::default();
    // For URLs like "builtin://component_name", the host is "component_name" and path is ""
    registry.register("openai", "", OpenAIComponent::new("gpt-3.5-turbo"));
    registry.register("create_messages", "", CreateMessagesComponent);
    registry.register("eval", "", EvalComponent::new());
    registry.register("load_file", "", LoadFileComponent);
    registry.register("put_blob", "", PutBlobComponent::new());
    registry.register("get_blob", "", GetBlobComponent::new());
    registry
});

pub fn get_component(component: &Component) -> Result<Arc<DynBuiltinComponent<'_>>> {
    // For builtin components, use the builtin name as the host
    // For other components, use the actual host and path
    let key = if component.is_builtin() {
        (
            component.builtin_name().map(|s| s.to_string()),
            String::new(),
        )
    } else {
        component.key()
    };

    let builtin_component = REGISTRY
        .components
        .get(&key)
        .ok_or_else(|| PluginError::UnknownComponent(component.clone()))?;

    Ok(builtin_component.clone())
}

pub fn list_components() -> Vec<Component> {
    REGISTRY
        .components
        .keys()
        .map(|(host, path)| {
            if let Some(host) = host {
                if path.is_empty() {
                    // This is a builtin component
                    Component::from_string(host)
                } else {
                    let url = format!("builtin://{host}/{path}");
                    Component::from_string(&url)
                }
            } else {
                // Fallback case
                Component::from_string(path)
            }
        })
        .collect()
}
