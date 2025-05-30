use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use stepflow_core::workflow::{Component, ComponentKey};
use stepflow_plugin::{PluginError, Result};

use crate::{
    BuiltinComponent, DynBuiltinComponent, eval::EvalComponent, load_file::LoadFileComponent,
    messages::CreateMessagesComponent, openai::OpenAIComponent,
};

#[derive(Default)]
struct Registry {
    components: HashMap<ComponentKey<'static>, Arc<DynBuiltinComponent<'static>>>,
}

impl Registry {
    pub fn register<C: BuiltinComponent + 'static>(
        &mut self,
        host: Option<&'static str>,
        path: &'static str,
        component: C,
    ) {
        let key = (host, path);
        let component = DynBuiltinComponent::boxed(component);
        let component = Arc::from(component);
        self.components.insert(key, component);
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    let mut registry = Registry::default();
    // For URLs like "builtins://component_name", the host is "component_name" and path is ""
    registry.register(Some("openai"), "", OpenAIComponent::new("gpt-3.5-turbo"));
    registry.register(Some("create_messages"), "", CreateMessagesComponent);
    registry.register(Some("eval"), "", EvalComponent::new());
    registry.register(Some("load_file"), "", LoadFileComponent);
    registry
});

pub fn get_component(component: &Component) -> Result<Arc<DynBuiltinComponent<'_>>> {
    // For URLs like "builtins://eval", the host is "eval" and path is ""
    // The components are registered with (host, path) keys
    let key = (component.host(), component.path());

    let component = REGISTRY
        .components
        .get(&key)
        .ok_or_else(|| PluginError::UnknownComponent(component.clone()))?;

    Ok(component.clone())
}
