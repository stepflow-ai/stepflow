// Licensed to the Apache Software Foundation (ASF) under one or more contributor license agreements.
// See the NOTICE file distributed with this work for additional information regarding copyright
// ownership.  The ASF licenses this file to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance with the License.  You may obtain a
// copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied.  See the License for the specific language governing permissions and limitations under
// the License.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};
use stepflow_core::workflow::Component;
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
    components: HashMap<&'static str, Arc<DynBuiltinComponent<'static>>>,
}

impl Registry {
    pub fn register<C: BuiltinComponent + 'static>(&mut self, path: &'static str, component: C) {
        let component = DynBuiltinComponent::boxed(component);
        let component = Arc::from(component);
        self.components.insert(path, component);
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    let mut registry = Registry::default();
    registry.register("openai", OpenAIComponent::new("gpt-3.5-turbo"));
    registry.register("create_messages", CreateMessagesComponent);
    registry.register("eval", EvalComponent::new());
    registry.register("load_file", LoadFileComponent);
    registry.register("put_blob", PutBlobComponent::new());
    registry.register("get_blob", GetBlobComponent::new());
    registry
});

pub fn get_component(component: &Component) -> Result<Arc<DynBuiltinComponent<'_>>> {
    let name = component
        .builtin_name()
        .ok_or(PluginError::UnknownComponent(component.clone()))?;
    let builtin_component = REGISTRY
        .components
        .get(name)
        .ok_or_else(|| PluginError::UnknownComponent(component.clone()))?;

    Ok(builtin_component.clone())
}

pub fn components() -> impl Iterator<Item = Arc<DynBuiltinComponent<'static>>> {
    REGISTRY.components.values().cloned()
}
